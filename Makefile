.PHONY: build run-digest run-digest-skill clean clean-full

LIST ?= lkml

build:
	cargo build --release

run-digest: build
	./target/release/lkml-digest --format compact -l $(LIST)

run-digest-skill: build
	claude -p "/lkml-digest zh $(LIST) 24h"

clean:
	cargo clean

clean-full: clean
	rm -rf "$${XDG_CACHE_HOME:-$$HOME/.cache}/lkml-tools"
