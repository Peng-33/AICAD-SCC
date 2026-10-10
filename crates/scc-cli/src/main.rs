use scc_cli::atomic_write::{atomic_write, AtomicPair, WriteOptions};
use scc_cli::compile;
use scc_cli::execute;
use scc_cli::inspect;
use scc_cli::output;
use scc_cli::solve;
use scc_cli::validate;
use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_geometry::ExportFormat;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use clap::{error::ErrorKind, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ExportFormatArgument {
    Svg,
    Dxf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SectionAxisArgument {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SectionSideArgument {
    Positive,
    Negative,
}

impl From<ExportFormatArgument> for ExportFormat {
    fn from(format: ExportFormatArgument) -> Self {
        match format {
            ExportFormatArgument::Svg => ExportFormat::Svg,
            ExportFormatArgument::Dxf => ExportFormat::Dxf,
        }
    }
}

#[derive(Parser)]
#[command(name = "scc", version, about = "Semantic Constraint Geometry Compiler")]
struct Cli {
    /// Overwrite existing output files without prompting.
    #[arg(long, global = true)]
    force: bool,

    /// Suppress normal success messages.
    #[arg(long, global = true)]
    quiet: bool,

    /// Output schema version. Use `legacy` (default) for unversioned JSON or
    /// `1.0` for the ADR-003 schema envelope.
    #[arg(long, global = true, value_name = "VERSION")]
    schema_version: Option<String>,

    /// Emit errors as a JSON document instead of human-readable text.
    #[arg(long, global = true)]
    json_errors: bool,

    /// Return exit code 2 when business conflicts are detected.
    #[arg(long, global = true)]
    fail_on_conflict: bool,

    #[command(subcommand)]
    command: Commands,
}

fn parse_schema_version(schema_version: Option<&str>) -> Result<bool, SccError> {
    match schema_version {
        None | Some("legacy") => Ok(false),
        Some("1.0") => Ok(true),
        Some(v) => Err(SccError::Structured(Box::new(StructuredError::new(
            "SCC-CLI-006",
            ErrorStage::Cli,
            format!(
                "unsupported schema version '{}', supported values are 'legacy' and '1.0'",
                v
            ),
        )))),
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a proposal into Constraint IR or a conflict report.
    Compile {
        input: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Execute a legacy or envelope Constraint IR.
    Execute {
        input: String,
        #[arg(short, long)]
        mutation: Option<String>,
        #[arg(short, long)]
        trace: Option<String>,
    },
    /// Validate a proposal and emit all static diagnostics.
    Validate { input: String },
    /// Inspect a constraint IR file and print a summary.
    Inspect { input: String },
    /// Solve a legacy or envelope GraphState into a GeometryModel.
    Solve {
        input: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Export a legacy or envelope GeometryModel as SVG or DXF.
    Export {
        input: String,
        #[arg(short, long, value_enum)]
        format: ExportFormatArgument,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// AICAD engineering intent bridge (DSL / tool calls / context / manifest).
    Engineering {
        #[command(subcommand)]
        command: EngineeringCommands,
    },
}

#[derive(Subcommand)]
enum EngineeringCommands {
    /// Emit the provider-neutral tool manifest (37 typed tools from scc-tools).
    Manifest,
    /// Apply DSL text to a fresh model; `--solve` also lowers to geometry.
    /// Output is the model JSON (or `{model, proposal, geometry}` with `--solve`).
    Dsl {
        file: String,
        #[arg(long)]
        solve: bool,
    },
    /// Apply a batch of tool calls (JSON) to a model or a fresh one; output the
    /// resulting model JSON (with `--solve`, also lower to geometry).
    Tools {
        file: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        solve: bool,
    },
    /// Solve a source model once and emit the complete derived snapshot.
    Snapshot { file: String },
    /// Derive the read-only LLM context view from a model JSON file.
    Context { file: String },
    /// Derive the read-only workbench view (UI panels) from a model JSON file.
    Workbench { file: String },
    /// Build the Phase 8 presentation drawing (SVG + sheet metadata) from a
    /// solved model JSON file; stdout is a single JSON document.
    Drawing { file: String },
    /// Build the formal Phase 9 Front/Top/Left presentation drawing (SVG +
    /// sheet metadata) from a spatial-ready model JSON file; stdout is a
    /// single JSON document.
    DrawingThreeView { file: String },
    /// Build Front/Top/Left and a real OCP-derived principal Section A-A.
    DrawingSection {
        file: String,
        #[arg(long, value_enum)]
        axis: SectionAxisArgument,
        #[arg(long)]
        offset_mm: f64,
        #[arg(long, value_enum, default_value = "positive")]
        view_side: SectionSideArgument,
    },
    /// Validate and serialize a source-only `.aicad.json` project.
    ProjectSave {
        file: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Validate and canonicalize a source-only `.aicad.json` project.
    ProjectOpen { file: String },
    /// Emit the exact STEP bytes accepted by the CadQuery/reimport validator.
    ExportStep { file: String },
}

enum RunOutcome {
    Success,
    Conflict { report_path: String },
    Invalid,
}

fn run(cli: Cli) -> Result<RunOutcome, SccError> {
    let write_options = WriteOptions { force: cli.force };
    let envelope = parse_schema_version(cli.schema_version.as_deref())?;
    let machine_conflict = cli.json_errors && cli.fail_on_conflict;
    let quiet = cli.quiet;

    match cli.command {
        Commands::Compile { input, output } => {
            let content = std::fs::read_to_string(&input)?;
            let result = compile::compile(&content, envelope)?;

            if let Some(ir) = result.ir_json {
                let out_path = output.unwrap_or_else(|| {
                    Path::new(&input)
                        .with_extension("ir.json")
                        .to_string_lossy()
                        .into_owned()
                });
                atomic_write(Path::new(&out_path), ir.as_bytes(), write_options)?;
                output::print_success(quiet, &format!("compiled successfully: {}", out_path));
                Ok(RunOutcome::Success)
            } else if let Some(conflict) = result.conflict_json {
                let out_path = output.unwrap_or_else(|| {
                    Path::new(&input)
                        .with_extension("conflict.json")
                        .to_string_lossy()
                        .into_owned()
                });
                atomic_write(Path::new(&out_path), conflict.as_bytes(), write_options)?;
                if !machine_conflict {
                    output::print_success(quiet, &format!("conflicts detected: {}", out_path));
                }
                Ok(RunOutcome::Conflict {
                    report_path: out_path,
                })
            } else {
                Err(SccError::Structured(Box::new(StructuredError::new(
                    "SCC-CLI-009",
                    ErrorStage::Cli,
                    "compile completed without an IR or conflict report",
                ))))
            }
        }
        Commands::Execute {
            input,
            mutation,
            trace,
        } => {
            let content = std::fs::read_to_string(&input)?;

            let patch = if let Some(path) = mutation {
                let content = std::fs::read_to_string(&path)?;
                Some(serde_json::from_str(&content)?)
            } else {
                None
            };

            let result = execute::execute(&content, patch, envelope)?;
            let trace_path = trace.unwrap_or_else(|| {
                Path::new(&input)
                    .with_extension("trace.json")
                    .to_string_lossy()
                    .into_owned()
            });
            let state_path = Path::new(&trace_path)
                .with_extension("state.json")
                .to_string_lossy()
                .into_owned();

            let pair = AtomicPair::prepare(
                Path::new(&trace_path),
                result.trace_json.as_bytes(),
                Path::new(&state_path),
                result.state_json.as_bytes(),
            )?;
            pair.commit(write_options)?;

            output::print_success(quiet, &format!("execution trace: {}", trace_path));
            output::print_success(quiet, &format!("graph state: {}", state_path));
            Ok(RunOutcome::Success)
        }

        Commands::Validate { input } => {
            let content = std::fs::read_to_string(&input)?;
            let result = validate::validate(&content)?;
            if result.ok {
                output::print_success(quiet, &format!("proposal is valid: {}", input));
                Ok(RunOutcome::Success)
            } else {
                if cli.json_errors {
                    output::write_json_errors(&mut std::io::stderr(), &result.diagnostics)?;
                } else {
                    output::write_human_errors(&result.diagnostics);
                }
                Ok(RunOutcome::Invalid)
            }
        }

        Commands::Inspect { input } => {
            let content = std::fs::read_to_string(&input)?;
            let report = inspect::inspect(&content)?;
            let json = serde_json::to_string_pretty(&report)?;
            output::print_success(quiet, &json);
            Ok(RunOutcome::Success)
        }

        Commands::Solve { input, output } => {
            let content = std::fs::read_to_string(&input)?;
            let geometry_json = solve::solve(&content, envelope)?;
            let out_path = output.unwrap_or_else(|| {
                Path::new(&input)
                    .with_extension("geometry.json")
                    .to_string_lossy()
                    .into_owned()
            });
            atomic_write(
                Path::new(&out_path),
                geometry_json.as_bytes(),
                write_options,
            )?;
            output::print_success(quiet, &format!("geometry model: {}", out_path));
            Ok(RunOutcome::Success)
        }
        Commands::Export {
            input,
            format,
            output,
        } => {
            let content = std::fs::read_to_string(&input)?;
            let extension = match format {
                ExportFormatArgument::Svg => "svg",
                ExportFormatArgument::Dxf => "dxf",
            };
            let cad = scc_cli::export_geometry(&content, format.into())?;
            let out_path = output.unwrap_or_else(|| {
                Path::new(&input)
                    .with_extension(extension)
                    .to_string_lossy()
                    .into_owned()
            });
            atomic_write(Path::new(&out_path), cad.as_bytes(), write_options)?;
            output::print_success(quiet, &format!("CAD export: {}", out_path));
            Ok(RunOutcome::Success)
        }
        Commands::Engineering { command } => match command {
            EngineeringCommands::Manifest => {
                let json = scc_cli::engineering::tool_manifest()?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::Dsl { file, solve } => {
                let dsl = std::fs::read_to_string(&file)?;
                let json = if solve {
                    scc_cli::engineering::dsl_solve(&dsl)?
                } else {
                    scc_cli::engineering::dsl_apply(&dsl)?
                };
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::Tools { file, model, solve } => {
                let calls = std::fs::read_to_string(&file)?;
                let model_json = match model {
                    Some(path) => Some(std::fs::read_to_string(&path)?),
                    None => None,
                };
                let json = scc_cli::engineering::tools_apply(&calls, model_json.as_deref(), solve)?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::Snapshot { file } => {
                let model_json = std::fs::read_to_string(&file)?;
                let json = scc_cli::engineering::snapshot(&model_json)?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::Context { file } => {
                let model_json = std::fs::read_to_string(&file)?;
                let json = scc_cli::engineering::context(&model_json)?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::Workbench { file } => {
                let model_json = std::fs::read_to_string(&file)?;
                let json = scc_cli::engineering::workbench(&model_json)?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::Drawing { file } => {
                let model_json = std::fs::read_to_string(&file)?;
                let json = scc_cli::engineering::drawing(&model_json)?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::DrawingThreeView { file } => {
                let model_json = std::fs::read_to_string(&file)?;
                let json = scc_cli::engineering::drawing_three_view(&model_json)?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::DrawingSection {
                file,
                axis,
                offset_mm,
                view_side,
            } => {
                let model_json = std::fs::read_to_string(&file)?;
                let axis = match axis {
                    SectionAxisArgument::X => scc_cad_backend::ParametricSectionAxis::X,
                    SectionAxisArgument::Y => scc_cad_backend::ParametricSectionAxis::Y,
                    SectionAxisArgument::Z => scc_cad_backend::ParametricSectionAxis::Z,
                };
                let view_side = match view_side {
                    SectionSideArgument::Positive => {
                        scc_cad_backend::ParametricSectionViewSide::Positive
                    }
                    SectionSideArgument::Negative => {
                        scc_cad_backend::ParametricSectionViewSide::Negative
                    }
                };
                let json = scc_cli::engineering::drawing_section_a_a(
                    &model_json,
                    axis,
                    offset_mm,
                    view_side,
                )?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::ProjectSave {
                file,
                name,
                description,
            } => {
                let model_json = std::fs::read_to_string(&file)?;
                let json =
                    scc_cli::engineering::project_save(&model_json, &name, description.as_deref())?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::ProjectOpen { file } => {
                let project_json = std::fs::read_to_string(&file)?;
                let json = scc_cli::engineering::project_open(&project_json)?;
                println!("{}", json);
                Ok(RunOutcome::Success)
            }
            EngineeringCommands::ExportStep { file } => {
                let model_json = std::fs::read_to_string(&file)?;
                let artifact = scc_cli::engineering::export_step(&model_json)?;
                eprintln!(
                    "AICAD-STEP-META sha256={} size_bytes={}",
                    artifact.sha256, artifact.size_bytes
                );
                eprintln!(
                    "AICAD-STEP-STATUS manufacturing=passed build=passed reimport=passed equivalence={}",
                    if artifact.witness.equivalence.all_match { "passed" } else { "failed" }
                );
                let timings = &artifact.witness.timings_ms;
                eprintln!(
                    "AICAD-STEP-PERF lowering_ms={:.3} build_ms={:.3} serialization_ms={:.3} reimport_ms={:.3} equivalence_ms={:.3} total_ms={:.3}",
                    timings.manufacturing_lowering_ms,
                    timings.manufacturing_build_ms,
                    timings.step_serialization_ms,
                    timings.step_reimport_ms,
                    timings.equivalence_validation_ms,
                    timings.total_ms,
                );
                std::io::stdout().write_all(&artifact.bytes)?;
                Ok(RunOutcome::Success)
            }
        },
    }
}

fn handle_error(err: SccError, json_errors: bool) -> ExitCode {
    let errors = output::normalize_errors(err);
    if json_errors {
        if let Err(write_err) = output::write_json_errors(&mut std::io::stderr(), &errors) {
            eprintln!("failed to write json errors: {}", write_err);
        }
    } else {
        output::write_human_errors(&errors);
    }
    output::exit_code_for_errors(&errors)
}

fn main() -> ExitCode {
    let json_errors_requested = std::env::args_os().any(|arg| arg == "--json-errors");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let exit_code = error.exit_code();
                let _ = error.print();
                return ExitCode::from(exit_code as u8);
            }
            if json_errors_requested {
                let structured =
                    StructuredError::new("SCC-CLI-010", ErrorStage::Cli, error.to_string())
                        .with_recoverable(true)
                        .with_suggestion("check the command syntax with scc --help");
                if let Err(write_error) =
                    output::write_json_errors(&mut std::io::stderr(), &[structured])
                {
                    return handle_error(write_error, true);
                }
                return ExitCode::from(1);
            }
            let exit_code = error.exit_code();
            let _ = error.print();
            return ExitCode::from(exit_code as u8);
        }
    };
    let json_errors = cli.json_errors;
    let fail_on_conflict = cli.fail_on_conflict;

    match run(cli) {
        Ok(RunOutcome::Success) => ExitCode::SUCCESS,
        Ok(RunOutcome::Conflict { report_path }) => {
            if fail_on_conflict {
                if json_errors {
                    let error = StructuredError::new(
                        "SCC-CLI-009",
                        ErrorStage::Cli,
                        "business conflicts detected",
                    )
                    .with_path(report_path)
                    .with_recoverable(true)
                    .with_suggestion("review the generated conflict report");
                    if let Err(write_error) =
                        output::write_json_errors(&mut std::io::stderr(), &[error])
                    {
                        return handle_error(write_error, true);
                    }
                }
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            }
        }
        Ok(RunOutcome::Invalid) => ExitCode::from(1),
        Err(e) => handle_error(e, json_errors),
    }
}
