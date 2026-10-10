import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { ServiceError } from "./scc-cli.mjs";

export const EXAMPLES = [
  { id: "polygon", name: "Triangular profile", description: "A fully constrained polygon with an editable mutation patch.", proposal: "polygon.proposal.json", mutation: "polygon.mutation.json" },
  { id: "six-hole-flange", name: "Six-hole flange top view", description: "Concentric Ø160/Ø90/Ø60 circles and six Ø14 holes on a Ø120 PCD; unsupported manufacturing requirements remain metadata.", proposal: "six-hole-flange.proposal.json" },
  { id: "linear-array", name: "Linear point array", description: "Three deterministic array instances spaced along the x axis.", proposal: "linear-array.proposal.json" },
  { id: "centered-plate", name: "Centered plate", description: "Rectangle and circle geometry driven by anchors and center constraints.", proposal: "centered-plate.proposal.json" },
  { id: "conflict", name: "Conflict review", description: "A proposal that opens the human-readable conflict workflow.", proposal: "bracket.conflict.proposal.json" },
];

export async function loadExample(repoRoot, id) {
  const example = EXAMPLES.find((candidate) => candidate.id === id);
  if (!example) throw new ServiceError(404, "SCC-UI-EXAMPLE-001", "Unknown example");
  const proposal = JSON.parse(await readFile(join(repoRoot, "examples", example.proposal), "utf8"));
  const mutation = example.mutation
    ? JSON.parse(await readFile(join(repoRoot, "examples", example.mutation), "utf8"))
    : [];
  return { ...example, proposal, mutation };
}
