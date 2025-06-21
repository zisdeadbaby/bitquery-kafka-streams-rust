#!/bin/bash

# Unified Deployment Script for Zolca Operations
# This script provides a configurable deployment framework for all components
# Usage: ./deploy.sh <component> <environment> [options]

set -euo pipefail

# --- Configuration ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CONFIG_DIR="$SCRIPT_DIR/configs"
TEMPLATE_DIR="$SCRIPT_DIR/templates"

# Default values (can be overridden by config files or environment variables)
DEFAULT_APP_DIR="/opt/zolca"
DEFAULT_APP_USER="zolca"
DEFAULT_GIT_REPO_URL="${GIT_REPO_URL:-https://github.com/yourusername/zolca-ops.git}"
DEFAULT_GIT_BRANCH="${GIT_BRANCH:-main}"
DEFAULT_RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"

# --- Helper Functions ---
log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] [INFO] $1"
}

error_exit() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] [ERROR] $1" >&2
    exit 1
}

usage() {
    cat << EOF
Usage: $0 <component> <environment> [options]

Components:
  ops-node          Deploy the HFT ops-node
  sdk               Deploy SDK examples/tests
  monitoring        Deploy monitoring stack

Environments:
  development       Local development setup
  staging           Staging environment
  production        Production environment

Options:
  --config FILE     Override default config file
  --app-dir DIR     Installation directory (default: $DEFAULT_APP_DIR)
  --user USER       Application user (default: $DEFAULT_APP_USER)
  --skip-deps       Skip dependency installation
  --dry-run         Show what would be done without executing

Examples:
  $0 ops-node production --config frankfurt-vps.conf
  $0 sdk development --dry-run
EOF
}

# --- Load Configuration ---
load_config() {
    local component="$1"
    local environment="$2"
    local config_file="${3:-$CONFIG_DIR/${component}-${environment}.conf}"
    
    if [[ -f "$config_file" ]]; then
        log "Loading configuration from $config_file"
        # shellcheck source=/dev/null
        source "$config_file"
    else
        log "No specific config found for $component-$environment, using defaults"
    fi
}

# --- System Optimization ---
optimize_system() {
    log "Applying system optimizations..."
    
    # Network optimizations
    cat << EOF > /tmp/zolca-network-optimizations.conf
# Network optimizations for low-latency trading
net.core.rmem_default = 262144
net.core.rmem_max = 16777216
net.core.wmem_default = 262144
net.core.wmem_max = 16777216
net.core.netdev_max_backlog = 5000
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216
net.ipv4.tcp_congestion_control = bbr
net.ipv4.tcp_fastopen = 3
net.core.busy_read = 50
net.core.busy_poll = 50
EOF

    if [[ "$DRY_RUN" != "true" ]]; then
        sudo cp /tmp/zolca-network-optimizations.conf /etc/sysctl.d/99-zolca.conf
        sudo sysctl -p /etc/sysctl.d/99-zolca.conf
    fi
    
    # CPU governor
    if [[ "$DRY_RUN" != "true" ]] && command -v cpupower >/dev/null; then
        sudo cpupower frequency-set -g performance || log "Could not set CPU governor"
    fi
}

# --- Component-specific deployment functions ---
deploy_ops_node() {
    log "Deploying ops-node component..."
    
    # Build the ops-node
    if [[ "$DRY_RUN" != "true" ]]; then
        cd "$PROJECT_ROOT/trading/hft/ops-node"
        cargo build --release
        
        # Install binary
        sudo mkdir -p "$APP_DIR/bin"
        sudo cp target/release/ops-node "$APP_DIR/bin/"
        sudo chown "$APP_USER:$APP_USER" "$APP_DIR/bin/ops-node"
        sudo chmod +x "$APP_DIR/bin/ops-node"
    fi
    
    # Install configuration
    if [[ -f "$TEMPLATE_DIR/ops-node.toml.template" ]]; then
        envsubst < "$TEMPLATE_DIR/ops-node.toml.template" > /tmp/ops-node.toml
        if [[ "$DRY_RUN" != "true" ]]; then
            sudo mkdir -p "$APP_DIR/config"
            sudo cp /tmp/ops-node.toml "$APP_DIR/config/"
            sudo chown "$APP_USER:$APP_USER" "$APP_DIR/config/ops-node.toml"
        fi
    fi
}

deploy_sdk() {
    log "Deploying SDK examples and tests..."
    
    if [[ "$DRY_RUN" != "true" ]]; then
        cd "$PROJECT_ROOT"
        cargo build --release --workspace
        cargo test --workspace
    fi
}

deploy_monitoring() {
    log "Deploying monitoring stack..."
    # Placeholder for monitoring deployment
    log "Monitoring deployment not yet implemented"
}

# --- Main execution ---
main() {
    local component=""
    local environment=""
    local config_file=""
    local app_dir="$DEFAULT_APP_DIR"
    local app_user="$DEFAULT_APP_USER"
    local skip_deps=false
    local dry_run=false
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --config)
                config_file="$2"
                shift 2
                ;;
            --app-dir)
                app_dir="$2"
                shift 2
                ;;
            --user)
                app_user="$2"
                shift 2
                ;;
            --skip-deps)
                skip_deps=true
                shift
                ;;
            --dry-run)
                dry_run=true
                shift
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            -*)
                error_exit "Unknown option: $1"
                ;;
            *)
                if [[ -z "$component" ]]; then
                    component="$1"
                elif [[ -z "$environment" ]]; then
                    environment="$1"
                else
                    error_exit "Too many positional arguments"
                fi
                shift
                ;;
        esac
    done
    
    # Validate required arguments
    if [[ -z "$component" ]] || [[ -z "$environment" ]]; then
        error_exit "Component and environment are required. Use --help for usage information."
    fi
    
    # Set global variables
    APP_DIR="$app_dir"
    APP_USER="$app_user"
    DRY_RUN="$dry_run"
    
    # Load configuration
    load_config "$component" "$environment" "$config_file"
    
    if [[ "$dry_run" == "true" ]]; then
        log "DRY RUN MODE - No changes will be made"
    fi
    
    # System setup
    if [[ "$skip_deps" != "true" ]]; then
        optimize_system
    fi
    
    # Deploy specific component
    case "$component" in
        ops-node)
            deploy_ops_node
            ;;
        sdk)
            deploy_sdk
            ;;
        monitoring)
            deploy_monitoring
            ;;
        *)
            error_exit "Unknown component: $component"
            ;;
    esac
    
    log "Deployment of $component for $environment completed successfully"
}

# Run main function with all arguments
main "$@"
