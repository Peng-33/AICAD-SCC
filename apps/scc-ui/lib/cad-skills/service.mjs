import { buildCadSkillPlan } from "./router.mjs";

export async function createCadSkillPlan({ adapter, skills, mode, text, model = null, modelFingerprint = null }) {
  const [context, manifest] = await Promise.all([
    mode === "edit" ? adapter.engineeringContext(JSON.stringify(model)) : Promise.resolve(null),
    adapter.engineeringManifest(),
  ]);
  return {
    plan: buildCadSkillPlan({ skills, mode, text, model, context, manifest, modelFingerprint }),
    context,
    manifest,
  };
}
