# Shortcuts for common tasks. See CLAUDE.md / README for details.
#
#   make          build the release binary and Campfire.app -> target/release/bundle
#   make install  same, then copy the app into /Applications
#   make run      run in development
#   make test     run the unit tests

.PHONY: bundle install run test

bundle:
	./scripts/bundle-macos.sh

install:
	./scripts/bundle-macos.sh --install

run:
	cargo run

test:
	cargo test
