# Security policy

AICAD 0.4.0-alpha.1 is an alpha engineering snapshot. It is not a production
CAD system and it is not a place to store secrets.

## Supported surface

There is no long-term supported release yet. Fixes apply to the current
public snapshot once a public repository exists. The private backup
repository is not a disclosure channel.

## Report a vulnerability

Do not open a public issue, discussion, or pull request that contains:

- an API key, token, password, or private key
- a working exploit against a third-party service
- personal data from the development machines that produced this snapshot

Until the public repository enables private vulnerability reporting, contact
the repository owner privately and wait for an acknowledgement. This tree
does not publish a personal email address for that contact.

If you find material in this snapshot that looks like a credential or a
private path, say where the file is. Do not paste the secret value.

## What the software already tries to do

- Intent provider keys stay in the Node process memory. The browser project
  file, drawings, and STEP artifact are not a key store.
- The Compose `ui` service binds `127.0.0.1:4173`, drops all capabilities,
  and uses a read-only root filesystem.
- Formal STEP is refused on the compatibility CAD route (`SCC-CAD-010`).
- The model fails closed on unsupported geometry instead of inventing a solid.

These controls are not a security certification.

## Out of scope

- Vulnerabilities in Rust, Node.js, CadQuery, OCP, Open CASCADE, Three.js,
  or Docker that are not triggered by AICAD's own code. Report those upstream.
- Missing CAD features. Use a normal issue for those.
