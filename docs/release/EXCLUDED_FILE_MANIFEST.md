# Excluded file manifest

Private baseline: `0568fac256178ab9b344995c1eab91861c292a7d`, 568 tracked files.
These tracked paths are not in the public snapshot. The original repository
still has them. Nothing here was deleted there.

| Path | Reason |
| --- | --- |
| `.agent/DECISIONS.md` | Internal agent state |
| `.agent/STATE.md` | Internal agent state |
| `tasks/plan.md` | Internal task notes |
| `tasks/todo.md` | Internal task notes |
| `docs/plans/AICAD-工程化架构技术方案.md` | Internal planning note |
| `docs/plans/AICAD-愿景差距诊断报告.md` | Internal diagnosis note |
| `docs/plans/AICAD-操作界面诊断报告.md` | Internal diagnosis note |
| `docs/plans/AICAD-混乱诊断报告.md` | Internal diagnosis note |
| `docs/plans/AICAD-项目介绍与架构说明.md` | Internal planning note |
| `docs/testing/engineering-ui-consolidation/verification-report.md` | Agent working-tree dump. The screenshot beside it is kept. |
| `SCC_Architecture_Compliance_Report.docx` | Office export. A Markdown report of the same era remains where it exists. |
| `SCC_Architecture_Gap_Report.docx` | Office export |
| `SCC_Architecture_Specification.docx` | Office export |
| `SCC_Milestone2_Acceptance_Report.docx` | Office export. The Markdown acceptance report remains. |
| `SCC_Milestone2_Complete_Architecture_Review_Report.docx` | Office export |
| `SCC_Required_Fix_List.docx` | Office export |
| `SCC_Risk_Assessment.docx` | Office export |
| `SCC_UI_UX_Design_Specification_EN.docx` | Office export not opened by the test suite |

Not excluded, after a review:

- `SCC_UI_UX_Design_Specification.docx` is a test fixture. See the completeness note.
- ADRs, acceptance reports, and crate tests stay. Absolute host paths in those reports were rewritten to `<repo>` or `<home>`, or to a relative image link.
- Historical reports that say `scc 0.3.0` were not edited into the new version.

Also not copied, because they are not source:

- the private `.git` directory and its 134 commits
- `target/` and `node_modules/`

The public snapshot is a new tree. It does not contain the private commit history.
