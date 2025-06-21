#!/bin/bash

# OpsNode Deployment Script
# This script provides a basic framework for building and running the ops-node.
# It includes common system optimizations for Linux.
# Customize paths, user, and specific commands as needed for your environment.

# --- Configuration ---
APP_DIR="/opt/ops-node" # Target directory for deployment
APP_USER="opsnode_user"  # Dedicated user to run the application
GIT_REPO_URL="https://your-git-repo/ops-node.git" # Replace with your actual Git repo URL
GIT_BRANCH="main" # Or your deployment branch
RUST_TOOLCHAIN="stable" # Or "nightly" if specific features are needed

# Environment variables for the application (can also be in a .env file)
export OPS_NODE_CONFIG="${APP_DIR}/config/ops-node.toml"
export RUST_LOG="info,ops_node=debug" # Adjust log levels as needed
# export YELLOWSTONE_API_KEY="your_api_key_here" # Example: set directly or via systemd EnvironmentFile
# export JITO_AUTH_TOKEN="your_jito_token_here" # Example

# --- Helper Functions ---
log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] [INFO] $1"
}

error_exit() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] [ERROR] $1" >&2
    exit 1
}

# --- Pre-flight Checks ---
log "Starting OpsNode deployment script..."

if [[ $EUID -ne 0 ]]; then
   error_exit "This script must be run as root or with sudo privileges."
fi

# Check for essential commands (git, cargo, rustc)
command -v git >/dev/null 2>&1 || error_exit "git is not installed. Please install git."
command -v cargo >/dev/null 2>&1 || error_exit "cargo is not installed. Please install Rust and Cargo (rustup.rs)."
command -v rustc >/dev/null 2>&1 || error_exit "rustc is not installed. Please install Rust and Cargo."

# --- System Optimizations (Linux) ---
# These are general recommendations. Test thoroughly and adjust for your specific hardware and workload.
# Some settings require a reboot or may not be applicable to all Linux distributions/kernels.
apply_system_optimizations() {
    log "Applying system optimizations..."

    # Network: Increase buffer sizes (example values, tune for your system)
    sysctl -w net.core.rmem_max=33554432 # Max receive buffer size
    sysctl -w net.core.wmem_max=33554432 # Max send buffer size
    sysctl -w net.core.rmem_default=16777216
    sysctl -w net.core.wmem_default=16777216
    sysctl -w net.core.netdev_max_backlog=10000
    sysctl -w net.ipv4.tcp_rmem="4096 87380 16777216"
    sysctl -w net.ipv4.tcp_wmem="4096 65536 16777216"

    # TCP settings for low latency (example)
    sysctl -w net.ipv4.tcp_congestion_control=bbr # Or cubic, or other high-performance options
    sysctl -w net.ipv4.tcp_notsent_lowat=16384 # If supported and beneficial
    sysctl -w net.ipv4.tcp_low_latency=1 # May not be available on all kernels

    # Kernel scheduler settings (example, for latency-sensitive workloads)
    # These require careful tuning and understanding.
    # sysctl -w kernel.sched_latency_ns=2000000 # Target latency for CFS tasks (e.g., 2ms)
    # sysctl -w kernel.sched_min_granularity_ns=250000 # Min preemption granularity (e.g., 0.25ms)
    # sysctl -w kernel.sched_wakeup_granularity_ns=300000 # Wakeup granularity (e.g., 0.3ms)

    # CPU Governor: Set to 'performance' for dedicated cores if ops-node is pinned
    # This is often done via tuned, cpupower, or directly.
    # Example for core 2-7 (ensure these cores are intended for the app):
    # for i in {2..7}; do
    #   if [ -f /sys/devices/system/cpu/cpu$i/cpufreq/scaling_governor ]; then
    #     echo performance > /sys/devices/system/cpu/cpu$i/cpufreq/scaling_governor
    #     log "Set CPU $i governor to performance."
    #   fi
    # done
    # Consider disabling CPU idle states (C-states) for pinned cores if ultra-low latency is critical.

    # Huge Pages (if app is configured to use them)
    # Example: Allocate 512 2MB huge pages (1GB total)
    # Check current value: cat /proc/sys/vm/nr_hugepages
    # DesiredHugePages=512
    # CurrentHugePages=$(cat /proc/sys/vm/nr_hugepages)
    # if [ "$CurrentHugePages" -lt "$DesiredHugePages" ]; then
    #   echo $DesiredHugePages > /proc/sys/vm/nr_hugepages
    #   log "Set nr_hugepages to $DesiredHugePages. A reboot might be needed for changes to fully apply or if memory is fragmented."
    # fi
    # Ensure hugetlbfs is mounted: mount -t hugetlbfs none /mnt/huge (add to /etc/fstab for persistence)

    log "System optimizations applied (some may require reboot or further config)."
}

# --- Application Setup ---
setup_application_environment() {
    log "Setting up application environment at ${APP_DIR}..."

    # Create application user if it doesn't exist
    if ! id -u "${APP_USER}" >/dev/null 2>&1; then
        log "Creating application user: ${APP_USER}"
        useradd -r -m -d "${APP_DIR}" -s /bin/bash "${APP_USER}" || error_exit "Failed to create user ${APP_USER}."
    else
        log "Application user ${APP_USER} already exists."
        usermod -d "${APP_DIR}" "${APP_USER}" # Ensure home directory is correct
    fi

    # Create directories
    mkdir -p "${APP_DIR}/source" "${APP_DIR}/config" "${APP_DIR}/logs" "${APP_DIR}/data"
    chown -R "${APP_USER}:${APP_USER}" "${APP_DIR}"
    chmod -R 750 "${APP_DIR}" # User rwx, Group rx, Other ---

    # TODO: Place your ops-node.toml configuration in ${APP_DIR}/config/ops-node.toml
    # Example:
    # if [ ! -f "${APP_DIR}/config/ops-node.toml" ]; then
    #   log "Configuration file not found. Please create ${APP_DIR}/config/ops-node.toml"
    #   # You could copy a default template here if you have one in the script's directory
    #   # cp ./ops-node.toml.template "${APP_DIR}/config/ops-node.toml"
    #   # chown "${APP_USER}:${APP_USER}" "${APP_DIR}/config/ops-node.toml"
    #   # error_exit "Configuration file missing."
    # fi

    # TODO: Place your Solana keypair JSON file where specified in ops-node.toml
    # Ensure it has correct permissions (e.g., readable only by APP_USER).
    # Example: chown opsnode_user:opsnode_user /path/to/keypair.json && chmod 400 /path/to/keypair.json

    log "Application environment setup complete."
}

# --- Build Application ---
build_application() {
    log "Building application from source..."

    # Switch to app user for git clone and build if possible, or adjust permissions after
    # For simplicity, this script runs git/cargo as root then chowns.
    # A more secure approach might use `sudo -u ${APP_USER} git clone ...` and `sudo -u ${APP_USER} cargo build ...`
    # if the user has passwordless sudo for these commands or if you handle Rust installation per-user.

    if [ -d "${APP_DIR}/source/ops-node" ]; then
        log "Existing source directory found. Pulling latest changes..."
        cd "${APP_DIR}/source/ops-node" || error_exit "Failed to cd to source directory."
        git checkout "${GIT_BRANCH}" || error_exit "Failed to checkout branch ${GIT_BRANCH}."
        git pull origin "${GIT_BRANCH}" || error_exit "Failed to pull latest changes."
    else
        log "Cloning repository ${GIT_REPO_URL} (branch ${GIT_BRANCH})..."
        git clone --branch "${GIT_BRANCH}" "${GIT_REPO_URL}" "${APP_DIR}/source/ops-node" || error_exit "Failed to clone repository."
        cd "${APP_DIR}/source/ops-node" || error_exit "Failed to cd to source directory."
    fi

    # Ensure correct Rust toolchain
    rustup show active-toolchain | grep -q "${RUST_TOOLCHAIN}" || rustup default "${RUST_TOOLCHAIN}"

    # Build with release profile, optimized for native CPU if possible
    # Add features as needed, e.g., --features "io_uring tsc"
    log "Starting Rust build (release profile)..."
    export RUSTFLAGS="-C target-cpu=native -C opt-level=3 -C lto=fat -C codegen-units=1"
    # For specific target: RUSTFLAGS="-C target-cpu=skylake ..." cargo build --release --target x86_64-unknown-linux-gnu

    # Assuming the ops-node project is at ${APP_DIR}/source/ops-node/ops-node (if workspace)
    # or ${APP_DIR}/source/ops-node (if it's the root of the repo)
    # Adjust path to Cargo.toml if your project structure is different.
    # This script assumes the structure is trading/hft/ops-node where hft is the workspace root.
    # So, we cd into trading/hft.
    PROJECT_ROOT_IN_REPO="trading/hft" # Path from git repo root to the workspace Cargo.toml
    cd "${APP_DIR}/source/ops-node/${PROJECT_ROOT_IN_REPO}" || error_exit "Failed to cd to project root ${PROJECT_ROOT_IN_REPO}."

    cargo build --release --package ops-node # Build only the ops-node package if it's a workspace member
    # If not a workspace, just `cargo build --release` from the ops-node project directory.

    # Copy binary to a known location
    mkdir -p "${APP_DIR}/bin"
    # Adjust path to binary based on workspace structure.
    # If workspace "hft" has member "ops-node", binary is target/release/ops-node
    cp "target/release/ops-node" "${APP_DIR}/bin/ops-node" || error_exit "Failed to copy built binary."
    chown -R "${APP_USER}:${APP_USER}" "${APP_DIR}/bin"
    chmod +x "${APP_DIR}/bin/ops-node"

    log "Build complete. Binary at ${APP_DIR}/bin/ops-node"
}

# --- Systemd Service Setup (Optional) ---
# Creates a systemd service file to manage the ops-node process.
setup_systemd_service() {
    log "Setting up systemd service for ops-node..."
    SERVICE_FILE="/etc/systemd/system/ops-node.service"

    # Real-time priority settings (chrt) require appropriate permissions/capabilities.
    # CPUAffinity should match isolated cores if pinning is used.
    # LimitMEMLOCK=infinity is important if using mlockall or large mmaped regions with MAP_LOCKED.
    cat > "${SERVICE_FILE}" << EOF
[Unit]
Description=OpsNode High-Frequency Trading Bot
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${APP_USER}
Group=${APP_USER}
WorkingDirectory=${APP_DIR} # Or ${APP_DIR}/source/ops-node/trading/hft/ if binary expects to run from there

# Environment variables can be set here or via EnvironmentFile
# Environment="RUST_LOG=info,ops_node=debug"
# Environment="OPS_NODE_CONFIG=${APP_DIR}/config/ops-node.toml"
# EnvironmentFile=${APP_DIR}/.env # If you use a .env file

ExecStartPre=/bin/bash -c 'echo 3 > /proc/sys/vm/drop_caches' # Optional: Clear page cache before start
ExecStart=/usr/bin/chrt -f 90 ${APP_DIR}/bin/ops-node # Example: FIFO scheduling prio 90
# Or without chrt if not needed/problematic:
# ExecStart=${APP_DIR}/bin/ops-node

Restart=always
RestartSec=5s
TimeoutStopSec=30s # Time to wait for graceful shutdown

# Resource Limits
LimitNOFILE=1048576 # Max open files
LimitNPROC=65536   # Max processes
LimitMEMLOCK=infinity # Important for mlock / huge pages that are locked

# CPU Affinity (example for cores 2,3,4,5) - ensure these are isolated
# CPUAffinity=2 3 4 5

# OOM Score Adjustment (lower score makes it less likely to be killed by OOM killer)
OOMScoreAdjust=-500

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable ops-node.service
    log "Systemd service 'ops-node.service' created and enabled."
    log "Use 'sudo systemctl start ops-node' to start the service."
    log "Use 'sudo systemctl status ops-node' to check its status."
    log "Use 'sudo journalctl -u ops-node -f' to view logs."
}


# --- Main Execution ---
# Uncomment the steps you want to perform:

# apply_system_optimizations
# setup_application_environment
# build_application
# setup_systemd_service

log "Deployment script finished. Review logs and perform manual steps if needed."
log "Next steps might include:
1. Placing config (ops-node.toml) in ${APP_DIR}/config/
2. Placing Solana keypair and ensuring correct permissions.
3. Setting environment variables (e.g., API keys) if not in config.
4. Starting the service: sudo systemctl start ops-node
5. Monitoring logs: sudo journalctl -u ops-node -f"

exit 0
