.PHONY: api-schema

API_SCHEMA_OUT ?= openapi.yaml

api-schema:
	python scripts/generate_openapi.py $(API_SCHEMA_OUT)
