#!/usr/bin/env bash
set -euo pipefail

chmod +x ./build.sh
./build.sh

# /app/target is a cache mount and disappears once this RUN completes, so
# the runtime stage can't COPY --from=builder from it directly.
mkdir -p /app/dist
cp target/release/backend /app/dist/backend
