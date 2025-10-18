# BKG Docker Setup

This directory contains Docker configuration for running the complete BKG stack.

## Architecture

The stack consists of:
- **PostgreSQL** (port 5432): Database with RLS support
- **cave-daemon** (port 8080): Rust backend API server
- **web-admin** (port 3001): Next.js admin interface
- **web-app** (port 3000): Next.js user/namespace interface

## Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+

## Quick Start

### 1. Setup Environment

```bash
# Copy environment template
cp docker/.env.example docker/.env

# Edit .env with your settings (optional)
nano docker/.env
```

### 2. Build and Start

```bash
# From project root
make docker-build
make docker-up

# Or combined
make dev-up
```

### 3. Access Services

- **API**: http://localhost:8080
- **Admin UI**: http://localhost:3001
- **User UI**: http://localhost:3000
- **Health Check**: http://localhost:8080/healthz
- **Metrics**: http://localhost:8080/metrics

## Makefile Commands

| Command | Description |
|---------|-------------|
| `make docker-build` | Build all Docker images |
| `make docker-up` | Start all services in detached mode |
| `make docker-down` | Stop all services |
| `make docker-logs` | Show logs from all services |
| `make docker-logs-daemon` | Show logs from cave-daemon only |
| `make docker-logs-web` | Show logs from web services |
| `make docker-clean` | Stop and remove containers, volumes |
| `make docker-rebuild` | Clean rebuild and restart |
| `make docker-ps` | Show running containers |
| `make docker-exec-daemon` | Shell into cave-daemon container |
| `make docker-exec-postgres` | Execute psql in postgres |
| `make dev-up` | Build and start dev environment |
| `make dev-restart` | Restart dev environment |
| `make help` | Show all available commands |

## Development Workflow

### Viewing Logs

```bash
# All services
make docker-logs

# Specific service
docker-compose -f docker/docker-compose.yml logs -f cave-daemon
docker-compose -f docker/docker-compose.yml logs -f web-admin
docker-compose -f docker/docker-compose.yml logs -f postgres
```

### Database Access

```bash
# Connect to PostgreSQL
make docker-exec-postgres

# Or manually
docker-compose -f docker/docker-compose.yml exec postgres psql -U cave -d bkg_db
```

### Container Shell Access

```bash
# cave-daemon
make docker-exec-daemon

# postgres
make docker-exec-postgres
```

### Rebuilding After Code Changes

```bash
# Full rebuild
make docker-rebuild

# Or step by step
make docker-down
make docker-build
make docker-up
```

## Environment Variables

Configure via `docker/.env`:

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_PASSWORD` | `cave_dev_password` | PostgreSQL password |
| `RUST_LOG` | `info` | Rust logging level |
| `CAVE_OTEL_SAMPLING_RATE` | `1.0` | Telemetry sampling (0.0-1.0) |
| `CAVE_ENABLE_NAMESPACES` | `true` | Enable Linux namespaces |
| `CAVE_ENABLE_CGROUPS` | `true` | Enable cgroups v2 limits |
| `NEXT_PUBLIC_API_URL` | `http://localhost:8080` | API URL for web UIs |

## Volumes

Persistent data is stored in Docker volumes:

- `postgres_data`: PostgreSQL database files
- `cave_workspaces`: Sandbox workspace data
- `cave_data`: Cave daemon state and logs

### Inspecting Volumes

```bash
# List volumes
docker volume ls | grep bkg

# Inspect volume
docker volume inspect docker_postgres_data
```

### Backup Database

```bash
# Dump database
docker-compose -f docker/docker-compose.yml exec -T postgres \
  pg_dump -U cave bkg_db > backup-$(date +%Y%m%d).sql

# Restore database
docker-compose -f docker/docker-compose.yml exec -T postgres \
  psql -U cave -d bkg_db < backup-20251018.sql
```

## Troubleshooting

### Port Already in Use

```bash
# Check what's using port 8080
sudo lsof -i :8080

# Stop the service or change port in docker-compose.yml
```

### Services Not Starting

```bash
# Check service status
make docker-ps

# Check logs for errors
make docker-logs-daemon

# Restart specific service
docker-compose -f docker/docker-compose.yml restart cave-daemon
```

### Database Connection Issues

```bash
# Ensure postgres is healthy
docker-compose -f docker/docker-compose.yml ps postgres

# Check database logs
docker-compose -f docker/docker-compose.yml logs postgres

# Test connection
docker-compose -f docker/docker-compose.yml exec postgres \
  pg_isready -U cave -d bkg_db
```

### Clean Slate

```bash
# Remove everything including volumes
make docker-clean

# Start fresh
make dev-up
```

## Production Considerations

Before deploying to production:

1. **Secrets Management**: Use Docker secrets or external secret manager
2. **Disable Development Features**: Set `CAVE_DISABLE_ISOLATION=false`
3. **Configure Resource Limits**: Add memory/CPU limits to services
4. **Enable TLS**: Configure `BKG_TLS_CERT` and `BKG_TLS_KEY`
5. **Adjust Sampling**: Set `CAVE_OTEL_SAMPLING_RATE=0.1` (10%)
6. **Configure Backups**: Set up automated database backups
7. **Health Monitoring**: Connect `/healthz` and `/metrics` to monitoring
8. **Use Strong Passwords**: Change all default passwords
9. **Network Security**: Use Docker networks with proper isolation
10. **Review Security**: Follow `docs/security.md` threat matrix

## CI/CD Integration

```yaml
# Example GitHub Actions workflow
- name: Build Docker Images
  run: make docker-build

- name: Run Tests
  run: |
    make docker-up
    # Wait for services
    sleep 30
    # Run tests against http://localhost:8080
    
- name: Cleanup
  run: make docker-clean
  if: always()
```

## Additional Resources

- [BKG README](../README.md) - System architecture and specifications
- [API Documentation](../docs/api.md) - API endpoints and contracts
- [Security Guide](../docs/security.md) - Security policies and threat matrix
- [Operations Manual](../docs/operations.md) - Production operations guide
