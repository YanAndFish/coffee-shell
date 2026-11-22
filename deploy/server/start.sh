#!/bin/sh
# The backend image's entry point: bring the database up to date, add the catalogue rows
# that are missing, then serve. Every step is safe to repeat on each start:
# `prisma migrate deploy` applies only the migrations not yet recorded in the database (it
# never resets or drops anything), and the seed only inserts rows that are absent.
set -eu
cd "${COFFEE_APP_DIR:-/app}"
./node_modules/.bin/prisma migrate deploy --config prisma.config.mjs
node dist/seed.mjs
exec node dist/main.mjs
