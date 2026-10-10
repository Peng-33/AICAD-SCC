FROM rust:1.96.0-bookworm AS scc-builder

WORKDIR /source
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build --locked --release -p scc-cli

FROM python:3.12.11-slim-bookworm AS cad-builder

COPY apps/scc-ui/cad/requirements.txt /tmp/cad-requirements.txt
RUN python -m venv /opt/aicad-cad \
    && /opt/aicad-cad/bin/pip install --no-cache-dir --disable-pip-version-check -r /tmp/cad-requirements.txt

FROM node:24.18.1-bookworm-slim AS ui-deps

WORKDIR /deps
COPY apps/scc-ui/package.json apps/scc-ui/package-lock.json ./
RUN npm ci --omit=dev --no-audit --no-fund

FROM node:24.18.1-bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends -y \
        libgl1=1.6.0-1 \
        poppler-utils=22.12.0-2+deb12u3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/scc
COPY --from=scc-builder /source/target/release/scc /usr/local/bin/scc
COPY --from=cad-builder /usr/local /usr/local
COPY --from=cad-builder /opt/aicad-cad /opt/aicad-cad
COPY --from=ui-deps --chown=node:node /deps/node_modules ./apps/scc-ui/node_modules
COPY --chown=node:node examples ./examples
COPY --chown=node:node skills/cad ./skills/cad
COPY --chown=node:node crates/scc-manufacturing/tests/goldens/installation_plate_solid_spec.json ./crates/scc-manufacturing/tests/goldens/installation_plate_solid_spec.json
COPY --chown=node:node crates/scc-manufacturing/tests/goldens/polygon_solid_spec.json ./crates/scc-manufacturing/tests/goldens/polygon_solid_spec.json
COPY --chown=node:node crates/scc-manufacturing/tests/goldens/composite_solid_spec.json ./crates/scc-manufacturing/tests/goldens/composite_solid_spec.json
COPY --chown=node:node apps/scc-ui ./apps/scc-ui

ENV NODE_ENV=production \
    SCC_BIN=/usr/local/bin/scc \
    SCC_CAD_PYTHON=/opt/aicad-cad/bin/python \
    PYTHONPATH=/opt/scc/apps/scc-ui/cad \
    XDG_CACHE_HOME=/tmp/aicad-cache \
    SCC_UI_HOST=0.0.0.0 \
    SCC_UI_PORT=4173
USER node
EXPOSE 4173
HEALTHCHECK --interval=5s --timeout=3s --start-period=5s --retries=12 \
  CMD ["node", "-e", "fetch('http://127.0.0.1:4173/api/health').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))"]
CMD ["node", "apps/scc-ui/server.mjs"]
