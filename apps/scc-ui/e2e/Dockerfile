FROM mcr.microsoft.com/playwright:v1.60.0-noble

WORKDIR /tests
COPY --chown=pwuser:pwuser package.json ./
RUN npm install --omit=optional --no-audit --no-fund
COPY --chown=pwuser:pwuser browser.mjs ./
COPY --chown=pwuser:pwuser browser-g4.mjs ./
COPY --chown=pwuser:pwuser browser-g5.mjs ./
COPY --chown=pwuser:pwuser browser-g6b.mjs ./
COPY --chown=pwuser:pwuser browser-g7b.mjs ./
USER pwuser
CMD ["node", "browser.mjs"]
