.PHONY: api-schema

API_SCHEMA_OUT ?= openapi.yaml

api-schema:
	cargo run --bin export-openapi --quiet -- $(API_SCHEMA_OUT)
