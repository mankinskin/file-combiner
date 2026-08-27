#!/bin/bash

cd -- "$(dirname -- "${BASH_SOURCE[0]}")" || exit 1

(cd frontend && trunk build --release)
(cd backend && cargo build --release)
