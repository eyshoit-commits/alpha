#!/bin/bash
# BKG Docker Setup Script
# Quick setup for local development environment

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "🚀 BKG Docker Setup"
echo "===================="
echo ""

# Check prerequisites
check_prereqs() {
    echo "📋 Checking prerequisites..."
    
    if ! command -v docker &> /dev/null; then
        echo "❌ Docker is not installed. Please install Docker first."
        exit 1
    fi
    
    if ! command -v docker-compose &> /dev/null; then
        echo "❌ docker-compose is not installed. Please install docker-compose first."
        exit 1
    fi
    
    echo "✅ Docker: $(docker --version)"
    echo "✅ docker-compose: $(docker-compose --version)"
    echo ""
}

# Setup environment
setup_env() {
    echo "🔧 Setting up environment..."
    
    if [ ! -f "$SCRIPT_DIR/.env" ]; then
        echo "📝 Creating .env from template..."
        cp "$SCRIPT_DIR/.env.example" "$SCRIPT_DIR/.env"
        echo "✅ Created docker/.env"
        echo "   You can edit this file to customize your configuration."
    else
        echo "ℹ️  .env already exists, skipping..."
    fi
    echo ""
}

# Create necessary directories
create_dirs() {
    echo "📁 Creating directories..."
    
    mkdir -p "$PROJECT_ROOT/dev-workspaces"
    mkdir -p "$SCRIPT_DIR/ssl"
    
    echo "✅ Created development directories"
    echo ""
}

# Generate self-signed SSL certificates (for development)
generate_ssl() {
    if [ ! -f "$SCRIPT_DIR/ssl/cert.pem" ]; then
        echo "🔐 Generating self-signed SSL certificates for development..."
        
        openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
            -keyout "$SCRIPT_DIR/ssl/key.pem" \
            -out "$SCRIPT_DIR/ssl/cert.pem" \
            -subj "/C=US/ST=State/L=City/O=BKG/CN=localhost" \
            2>/dev/null || echo "⚠️  OpenSSL not found, skipping SSL generation"
        
        if [ -f "$SCRIPT_DIR/ssl/cert.pem" ]; then
            echo "✅ SSL certificates generated"
        fi
    else
        echo "ℹ️  SSL certificates already exist"
    fi
    echo ""
}

# Build and start services
start_services() {
    echo "🏗️  Building Docker images..."
    cd "$PROJECT_ROOT"
    make docker-dev-build
    
    echo ""
    echo "🚀 Starting services..."
    make docker-dev-up
    
    echo ""
    echo "⏳ Waiting for services to be healthy..."
    sleep 10
}

# Show status
show_status() {
    echo ""
    echo "📊 Service Status:"
    cd "$PROJECT_ROOT"
    make docker-ps
    
    echo ""
    echo "✅ BKG is ready!"
    echo ""
    echo "🌐 Access Points:"
    echo "   API:        http://localhost:8080"
    echo "   Health:     http://localhost:8080/healthz"
    echo "   Metrics:    http://localhost:8080/metrics"
    echo "   Admin UI:   http://localhost:3001"
    echo "   User UI:    http://localhost:3000"
    echo ""
    echo "📚 Useful Commands:"
    echo "   make docker-logs       # View all logs"
    echo "   make docker-logs-daemon   # View backend logs"
    echo "   make docker-ps         # Show container status"
    echo "   make docker-down       # Stop services"
    echo "   make help              # Show all commands"
    echo ""
    echo "📖 For more information, see docker/README.md"
}

# Main execution
main() {
    check_prereqs
    setup_env
    create_dirs
    generate_ssl
    start_services
    show_status
}

# Run main if script is executed directly
if [ "${BASH_SOURCE[0]}" -ef "$0" ]; then
    main
fi
