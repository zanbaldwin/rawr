# Build orchestration for the two-toolchain reality: cargo owns the
# binaries, npm owns crates/web/ui. Cargo never shells out to npm — a
# release build of rawr-web *fails* if `make assets` hasn't run.

NPM := npm --prefix crates/web/ui

.PHONY: require-node ui-install assets ui-dev ui-check types check deploy

require-node:
	@command -v npm >/dev/null || { echo >&2 'npm not found; install Node (see .nvmrc)'; exit 1; }

ui-install: require-node
	$(NPM) ci

assets: require-node
	$(NPM) run build

ui-dev: require-node
	$(NPM) run dev

ui-check: require-node
	$(NPM) run typecheck
	$(NPM) run test

# Regenerate the committed TypeScript bindings; CI-style drift check:
# `make types && git diff --exit-code -- crates/web/ui/src/types/generated`
types:
	cargo test -p rawr-web --features ts

check:
	cargo check --workspace
	cargo check -p rawr-web --no-default-features

# A local release deploy must never embed a stale frontend.
deploy: assets
	cargo deploy -p rawr-web
