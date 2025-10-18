.PHONY: api-schema docker-build docker-up docker-down docker-logs docker-clean docker-rebuild help
.PHONY: docker-dev-up docker-prod-up docker-prod-down

API_SCHEMA_OUT ?= openapi.yaml
DOCKER_COMPOSE = docker-compose -f docker/docker-compose.yml
DOCKER_COMPOSE_DEV = docker-compose -f docker/docker-compose.yml -f docker/docker-compose.dev.yml
DOCKER_COMPOSE_PROD = docker-compose -f docker/docker-compose.yml -f docker/docker-compose.prod.yml

# Default target
.DEFAULT_GOAL := help

# Generate OpenAPI schema
api-schema:
	python scripts/generate_openapi.py $(API_SCHEMA_OUT)

# Docker targets
docker-build: ## Build all Docker images
	$(DOCKER_COMPOSE) build

docker-up: ## Start all services
	$(DOCKER_COMPOSE) up -d

docker-down: ## Stop all services
	$(DOCKER_COMPOSE) down

docker-logs: ## Show logs from all services
	$(DOCKER_COMPOSE) logs -f

docker-logs-daemon: ## Show logs from cave-daemon
	$(DOCKER_COMPOSE) logs -f cave-daemon

docker-logs-web: ## Show logs from web services
	$(DOCKER_COMPOSE) logs -f web-admin web-app

docker-clean: ## Stop and remove all containers, networks, and volumes
	$(DOCKER_COMPOSE) down -v --remove-orphans

docker-rebuild: docker-clean docker-build docker-up ## Clean rebuild and restart all services

docker-ps: ## Show running containers
	$(DOCKER_COMPOSE) ps

docker-exec-daemon: ## Execute shell in cave-daemon container
	$(DOCKER_COMPOSE) exec cave-daemon /bin/sh

docker-exec-postgres: ## Execute psql in postgres container
	$(DOCKER_COMPOSE) exec postgres psql -U cave -d bkg_db

# Development targets
docker-dev-up: ## Start services with development overrides
	$(DOCKER_COMPOSE_DEV) up -d

docker-dev-build: ## Build services with development configuration
	$(DOCKER_COMPOSE_DEV) build

docker-dev-rebuild: ## Clean rebuild with development configuration
	$(DOCKER_COMPOSE_DEV) down -v
	$(DOCKER_COMPOSE_DEV) build
	$(DOCKER_COMPOSE_DEV) up -d

dev-up: docker-dev-build docker-dev-up ## Build and start development environment

dev-restart: docker-down docker-dev-up ## Restart development environment

# Production targets
docker-prod-build: ## Build services with production configuration
	$(DOCKER_COMPOSE_PROD) build

docker-prod-up: ## Start services with production configuration
	$(DOCKER_COMPOSE_PROD) up -d

docker-prod-down: ## Stop production services
	$(DOCKER_COMPOSE_PROD) down

docker-prod-logs: ## Show production logs
	$(DOCKER_COMPOSE_PROD) logs -f

docker-prod-restart: docker-prod-down docker-prod-up ## Restart production services

# Help target
help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-20s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
