# Frankfurt VPS Deployment Guide for ops-node

This guide details the setup and optimization of the `ops-node` application on a Frankfurt-based VPS, specifically tailored for low-latency trading operations connected to Solana RPC providers like RPCFast.

## VPS Details (Example)
- **IP**: `141.95.110.92` (Replace with your actual IP)
- **SSH User**: `ubuntu` (Or your default SSH user)
- **Provider**: OVHcloud Frankfurt (Or similar, steps might vary slightly)
- **Target Latency**: Aim for <0.2ms to RPCFast Frankfurt nodes.

## Section 1: Initial Server Setup

### 1.1. Connect to VPS & Update System
```bash
# Replace with your VPS IP and user
ssh ubuntu@141.95.110.92

# Switch to root user for system-wide changes
sudo su -

# Update package lists and upgrade installed packages
apt update && apt upgrade -y
```

### 1.2. Install Essential Dependencies
```bash
# Install build tools, network tools, performance analysis tools, etc.
apt install -y build-essential pkg-config libssl-dev git curl \
    linux-tools-common linux-tools-generic linux-tools-$(uname -r) \
    htop iotop nethogs sysstat numactl cpufrequtils ethtool ufw fail2ban
```

### 1.3. Install Rust
```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env # Add to .bashrc or .profile for persistence for root
echo 'source $HOME/.cargo/env' >> ~/.bashrc # For root user
# rustup default stable # Or your preferred toolchain (e.g., nightly if needed)
```

### 1.4. Create Dedicated Application User
```bash
APP_USER="opsnode" # Choose a username for the application
APP_HOME_DIR="/opt/${APP_USER}" # Home directory and application base

useradd -m -d "${APP_HOME_DIR}" -s /bin/bash "${APP_USER}"
# Consider adding to sudo group if needed for some initial setup steps by the user,
# but generally, the app itself should not run as sudo.
# usermod -aG sudo ${APP_USER}

# Setup directories for the application
mkdir -p "${APP_HOME_DIR}/app/source" "${APP_HOME_DIR}/app/config" "${APP_HOME_DIR}/app/logs" "${APP_HOME_DIR}/app/data"
chown -R "${APP_USER}:${APP_USER}" "${APP_HOME_DIR}"
chmod -R 750 "${APP_HOME_DIR}" # User rwx, Group rx
```
It's recommended to perform application build and run steps as this dedicated user.

### 1.5. (Optional) Setup SSH Key for Git & Secure SSH
- Generate SSH keys for the `opsnode` user if you need to clone private git repositories.
- Harden SSH: Disable password authentication, change default port, use Fail2Ban (installed above).

## Section 2: Kernel & System Optimizations (Run as root)

### 2.1. GRUB Configuration for CPU Isolation & Low Latency
Edit `/etc/default/grub`:
```bash
# Example: Isolate cores 2-7 (assuming 8-core CPU, leaving 0-1 for OS/IRQ)
# Adjust `isolcpus`, `nohz_full`, `rcu_nocbs` based on your CPU core count and desired isolation.
# `intel_pstate=disable` might be needed on some Intel CPUs to allow `cpufrequtils` to control frequency precisely.
# `mitigations=off` disables CPU security mitigations for max performance (SECURITY RISK! Understand implications).
# `idle=poll` can reduce sleep state latency but increases power consumption.
# `transparent_hugepage=madvise` is generally preferred over `always`.
sudo sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="\(.*\)"/GRUB_CMDLINE_LINUX_DEFAULT="\1 isolcpus=2-7 nohz_full=2-7 rcu_nocbs=2-7 irqaffinity=0,1 intel_idle.max_cstate=0 processor.max_cstate=0 idle=poll nmi_watchdog=0 audit=0 mitigations=off transparent_hugepage=madvise"/' /etc/default/grub

# For intel_pstate=disable (if needed, test without first):
# sudo sed -i 's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 intel_pstate=disable"/' /etc/default/grub

sudo update-grub
sudo reboot # Reboot is required for GRUB changes to take effect
```
**Note:** After reboot, verify isolated cores are not used by most processes: `ps -eLo psr | sort | uniq -c | sort -n`.

### 2.2. Sysctl Network & Kernel Tuning
Create `/etc/sysctl.d/99-ops-node-hft.conf`:
```ini
# Network optimizations for HFT
net.core.somaxconn = 4096
net.core.rmem_max = 268435456 # 256MB
net.core.wmem_max = 268435456 # 256MB
net.core.rmem_default = 67108864 # 64MB
net.core.wmem_default = 67108864 # 64MB
net.core.netdev_max_backlog = 65536
net.core.netdev_budget = 600
net.core.busy_read = 50 # Requires kernel support & socket option
net.core.busy_poll = 50 # Requires kernel support & socket option

# TCP optimizations
net.ipv4.tcp_congestion_control = bbr # Modern congestion control
net.ipv4.tcp_notsent_lowat = 16384 # Lower threshold for sending data
net.ipv4.tcp_low_latency = 1 # Hint for lower latency, may not be on all kernels
net.ipv4.tcp_timestamps = 0 # Disable for slight overhead reduction, can affect some diagnostics
net.ipv4.tcp_sack = 1
net.ipv4.tcp_window_scaling = 1
net.ipv4.tcp_fastopen = 3 # Enable TCP Fast Open for client and server
net.ipv4.tcp_rmem = 4096 87380 268435456
net.ipv4.tcp_wmem = 4096 65536 268435456
net.ipv4.tcp_mtu_probing = 1 # Enable MTU probing

# Kernel scheduler & VM optimizations
kernel.sched_latency_ns = 2000000 # Target latency for CFS (e.g. 2ms)
kernel.sched_min_granularity_ns = 250000 # Min preemption time (e.g. 0.25ms)
kernel.sched_wakeup_granularity_ns = 300000 # Wakeup granularity (e.g. 0.3ms)
# kernel.sched_rt_runtime_us = -1 # Allow RT tasks to run indefinitely (use with caution)
vm.swappiness = 0 # Avoid swapping at all costs
vm.dirty_ratio = 10
vm.dirty_background_ratio = 5
# vm.zone_reclaim_mode = 0 # Can help avoid NUMA reclaims

# Security (some disabled for performance - understand risks)
kernel.randomize_va_space = 0 # Disable ASLR for slight perf gain (security risk)
```
Apply settings:
```bash
sudo sysctl -p /etc/sysctl.d/99-ops-node-hft.conf
```

### 2.3. CPU Frequency & Power Management
```bash
# Set CPU governor to 'performance' for all cores (or just isolated ones)
for i in $(seq 0 $(($(nproc)-1))); do
    sudo cpufreq-set -c $i -g performance
done
# Verify: cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# (Optional) Disable CPU idle states (C-states) for isolated cores
# This reduces latency but significantly increases power consumption.
# Example for cores 2-7:
# for i in {2..7}; do
#   for j in /sys/devices/system/cpu/cpu$i/cpuidle/state*/disable; do
#     echo 1 | sudo tee $j > /dev/null
#   done
# done

# (Optional) Lock CPU frequency to maximum for isolated cores
# This prevents frequency scaling even within the 'performance' governor.
# for i in {2..7}; do
#   max_freq=$(cat /sys/devices/system/cpu/cpu$i/cpufreq/cpuinfo_max_freq)
#   sudo cpufreq-set -c $i --min $max_freq --max $max_freq
# done
# Verify: cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq
```

### 2.4. Huge Pages Configuration
If `ops-node` is configured to use huge pages (`enable_huge_pages = true` in config):
```bash
# Configure number of huge pages (e.g., 1024 * 2MB pages = 2GB)
# Adjust based on `memory_pool_size_mb` and other needs.
echo 'vm.nr_hugepages = 1024' | sudo tee -a /etc/sysctl.conf
sudo sysctl -p

# Mount hugetlbfs if not already (check /etc/fstab)
if ! grep -q '/mnt/huge' /proc/mounts; then
  sudo mkdir -p /mnt/huge
  sudo mount -t hugetlbfs nodev /mnt/huge
  # Add to /etc/fstab for persistence:
  echo "nodev /mnt/huge hugetlbfs defaults,gid=$(getent group opsnode | cut -d: -f3),mode=0770 0 0" | sudo tee -a /etc/fstab
  # Adjust gid to your APP_USER's group if different, and ensure APP_USER can access.
fi
# Verify: grep HugePages /proc/meminfo
```

### 2.5. Network Interface (NIC) Optimizations
Identify your primary network interface (e.g., `eth0`, `ens3`):
```bash
IFACE=$(ip route | grep default | awk '{print $5}')
echo "Optimizing network interface: $IFACE"

# 1. IRQ Affinity: Assign NIC interrupts to specific non-isolated cores (e.g., 0-1)
# This is complex and scriptable; `irqbalance` service often needs to be disabled first.
# sudo systemctl stop irqbalance
# sudo systemctl disable irqbalance
# Example: Use `set_irq_affinity.sh` script from kernel docs or tools like `RaptorAFK/set_irq_affinity`.
# For a simple approach (might not cover all IRQs, e.g. multiqueue NICs):
# for irq in $(grep $IFACE /proc/interrupts | awk '{print $1}' | sed 's/://g'); do
#   echo "Setting affinity for IRQ $irq ($IFACE) to CPU 0-1"
#   sudo bash -c "echo 3 > /proc/irq/$irq/smp_affinity" # 3 = 00000011 in binary (CPU0 and CPU1)
# done

# 2. Increase NIC Ring Buffers
# Check current: sudo ethtool -g $IFACE
# Set new values (example, max might depend on driver):
sudo ethtool -G $IFACE rx 4096 tx 4096

# 3. Offloading Features
# Check current: sudo ethtool -k $IFACE
# Enable common useful offloads if not already on:
sudo ethtool -K $IFACE gro on gso on tso on # Generic Receive/Send/TCP Segmentation Offload

# 4. (Optional) Consider Receive Packet Steering (RPS) / Receive Flow Steering (RFS)
# If not using full kernel bypass like AF_XDP and if isolated cores handle network processing.
# Example for RPS on core 2-7 for interface's queue 0:
# sudo bash -c "echo fc > /sys/class/net/$IFACE/queues/rx-0/rps_cpus" # fc = 11111100 (CPU2-7)
# sudo bash -c "echo 32768 > /proc/sys/net/core/rps_sock_flow_entries"
# sudo bash -c "echo 32768 > /sys/class/net/$IFACE/queues/rx-0/rps_flow_cnt"

# 5. (Advanced) Kernel Bypass (AF_XDP / DPDK)
# If `ops-node` supports AF_XDP (`use_af_xdp = true` feature), this requires specific NIC drivers,
# kernel versions, and application code. This guide does not cover full AF_XDP setup.
```

## Section 3: Application Deployment (Run as `opsnode` user)

Switch to the application user:
```bash
sudo su - ${APP_USER} # Replace ${APP_USER} with your chosen username, e.g., opsnode
cd "${APP_HOME_DIR}/app" # Navigate to the application base directory (e.g., /opt/opsnode/app)
```

### 3.1. Clone Application Repository
```bash
# Replace with your actual repository URL and branch
GIT_REPO_URL="https://github.com/YOUR_REPO/bitquery-solana-sdk.git" # Example
GIT_BRANCH="main" # Or your deployment branch

if [ -d "source/bitquery-solana-sdk" ]; then
  echo "Pulling latest changes..."
  cd source/bitquery-solana-sdk
  git checkout $GIT_BRANCH && git pull
  cd ../..
else
  echo "Cloning repository..."
  git clone --branch $GIT_BRANCH $GIT_REPO_URL source/bitquery-solana-sdk
fi
```

### 3.2. Configure Application
- Create/copy `ops-node.toml` to `${APP_HOME_DIR}/app/config/ops-node.toml`.
- **Critical**: Update `rpc_endpoint`, `yellowstone_endpoint`, `jito_endpoint` to your Frankfurt low-latency providers.
- **Critical**: Update `private_key_path` to the correct location of your Solana keypair JSON.
- Set API keys (`api_key` for Yellowstone, `jito_auth_token`) either directly in the TOML (less secure) or via environment variables (preferred).
- Adjust `cpu_cores` in `ops-node.toml` to match your isolated cores (e.g., `[2, 3, 4, 5, 6, 7]`).
- Enable performance features: `use_io_uring = true`, `enable_huge_pages = true` (if system is configured).

Example `ops-node.toml` snippet for Frankfurt:
```toml
[network]
rpc_endpoint = "https://frankfurt.rpc.YOURPROVIDER.com" # Low-latency RPC
yellowstone_endpoint = "grpc://frankfurt.yellowstone.YOURPROVIDER.com:443"
jito_endpoint = "grpc://frankfurt.jito.YOURPROVIDER.com:443" # Jito Frankfurt endpoint
connection_pool_size = 32 # Increased for potentially more connections
request_timeout_ms = 1500 # Reduced for faster timeout in low-latency env

[performance]
cpu_cores = [2, 3, 4, 5, 6, 7] # Match isolated cores
memory_pool_size_mb = 1024 # Larger pool if using huge pages extensively
max_concurrent_operations = 250
latency_threshold_ns = 500000  # 0.5ms
use_io_uring = true
enable_huge_pages = true
```

### 3.3. Set Environment Variables (Optional but Recommended for Secrets)
Create `${APP_HOME_DIR}/.env` (e.g., `/opt/opsnode/.env`):
```bash
export OPS_NODE_CONFIG="${APP_HOME_DIR}/app/config/ops-node.toml"
export RUST_LOG="info,ops_node=debug,h2=warn,hyper=warn,tower=warn" # Tune verbosity
export OPS_NODE_YELLOWSTONE_TOKEN="YOUR_YELLOWSTONE_API_KEY"
export OPS_NODE_JITO_TOKEN="YOUR_JITO_AUTH_TOKEN" # If applicable
# Add any other environment variables your application might need.
```
Source this file in the user's profile (`.bashrc` or `.profile`) or in the systemd service file.
```bash
echo 'source ${APP_HOME_DIR}/.env' >> ~/.bashrc # For opsnode user
source ~/.bashrc
```
Ensure `${APP_HOME_DIR}/.env` has strict permissions: `chmod 600 ${APP_HOME_DIR}/.env`.

### 3.4. Build Application
```bash
cd "${APP_HOME_DIR}/app/source/bitquery-solana-sdk/trading/hft" # Navigate to workspace dir

# Set RUSTFLAGS for optimized build targeting specific CPU architecture if known
# For OVH Frankfurt, CPUs are often Intel Xeon E-series (e.g., Skylake, Cascade Lake).
# Check `lscpu` or provider docs. `skylake` is a good general baseline for recent Intel.
# `native` targets the build machine's CPU specifically.
export RUSTFLAGS="-C target-cpu=native -C opt-level=3 -C lto=fat -C codegen-units=1 -C panic=abort"
# Add features if your Cargo.toml defines them for io_uring, tsc, af_xdp etc.
# cargo build --release --package ops-node --features "io_uring tsc"
cargo build --release --package ops-node

# Copy binary to a clean location
mkdir -p "${APP_HOME_DIR}/app/bin"
cp target/release/ops-node "${APP_HOME_DIR}/app/bin/"
```

## Section 4: Running and Managing `ops-node`

### 4.1. Systemd Service (Recommended for Production)
As root, create `/etc/systemd/system/ops-node.service`:
```ini
[Unit]
Description=OpsNode HFT Trading Bot
After=network-online.target systemd-sysctl.service # Ensure sysctl settings are applied
Wants=network-online.target

[Service]
Type=simple
User=${APP_USER} # e.g., opsnode
Group=${APP_USER} # e.g., opsnode
WorkingDirectory=${APP_HOME_DIR}/app # Or wherever the binary expects to run from

# Source environment variables if using a .env file
EnvironmentFile=-${APP_HOME_DIR}/.env # The '-' makes it non-fatal if file is missing

# Execute with real-time priority and CPU affinity (if cores are isolated)
# `chrt -f 90` requests FIFO scheduling at priority 90 (high).
# `taskset -c 2-7` pins to cores 2-7.
# Adjust core list (2-7) based on your `isolcpus` GRUB setting.
ExecStart=/usr/bin/taskset -c 2-7 /usr/bin/chrt -f 90 ${APP_HOME_DIR}/app/bin/ops-node
# If not using taskset/chrt, simply:
# ExecStart=${APP_HOME_DIR}/app/bin/ops-node

Restart=always # Or on-failure
RestartSec=5s
TimeoutStopSec=60s # Allow ample time for graceful shutdown

# Resource Limits
LimitNOFILE=1048576
LimitNPROC=65536
LimitMEMLOCK=infinity # Crucial if using mlock with huge pages

# OOM Score Adjustment (lower is less likely to be killed)
OOMScoreAdjust=-800

[Install]
WantedBy=multi-user.target
```
Reload systemd, enable and start the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable ops-node.service
sudo systemctl start ops-node.service

# Check status and logs:
sudo systemctl status ops-node.service
sudo journalctl -u ops-node -f -n 100 # Follow logs
```

### 4.2. Manual Execution (for Testing)
As the `opsnode` user:
```bash
cd "${APP_HOME_DIR}/app"
source "${APP_HOME_DIR}/.env" # Load environment variables

# Run with taskset/chrt for performance testing:
# Adjust core list (2-7) based on your `isolcpus` GRUB setting.
taskset -c 2-7 chrt -f 90 ./bin/ops-node
# Or simply:
# ./bin/ops-node
```

## Section 5: Performance Validation & Monitoring

### 5.1. Latency Testing
- Use `ping -c 100 -i 0.01 <your_rpc_provider_ip>` for raw network RTT.
- If `ops-node` has a built-in gRPC health check or latency test endpoint, use that.
- Monitor `op_latency_seconds` metrics from the `/metrics` endpoint.

### 5.2. CPU Usage & Affinity
- `htop`: Filter by `ops-node` process, check CPU core usage. Ensure it's running on pinned cores if configured.
- `taskset -cp $(pgrep ops-node)`: Verify CPU affinity of the running process.

### 5.3. Memory Usage
- `htop` or `ps aux | grep ops-node` for RES/RSS memory.
- `grep HugePages /proc/meminfo`: Check huge page usage if enabled.
- `numastat -m -p $(pgrep ops-node)`: Check NUMA memory locality if relevant.

### 5.4. Network Performance
- `ss -ti`: Show TCP connection info, including RTT, retransmissions.
- `nethogs $IFACE`: Monitor bandwidth usage per process.
- `ethtool -S $IFACE`: View NIC statistics (errors, drops, etc.).

### 5.5. Application Logs & Metrics
- Continuously monitor application logs: `sudo journalctl -u ops-node -f`.
- Scrape Prometheus metrics from `http://<VPS_IP>:9090/metrics`. Setup Grafana dashboards.

## Section 6: Security Hardening (Basic)
```bash
# Firewall setup (ufw)
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow ssh # Or your custom SSH port
sudo ufw allow 9090/tcp # For Prometheus metrics (if exposed publicly, consider auth/VPN)
# Allow other necessary outbound ports for RPC/gRPC if egress is restricted.
sudo ufw enable

# Fail2Ban (already installed) will monitor SSH logs by default.
# Check status: sudo systemctl status fail2ban
# View jails: sudo fail2ban-client status sshd

# Regular updates
# sudo apt update && sudo apt upgrade -y

# Disable unused services
# sudo systemctl disable bluetooth cups avahi-daemon # etc.
```

## Section 7: Troubleshooting Tips
- **High Latency**:
    - Verify CPU isolation and pinning (`taskset`, `htop`).
    - Check CPU frequency (`cpufreq-info`, `watch -n 0.5 "cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq"`).
    - Review network path (`mtr <rpc_ip>`), NIC stats (`ethtool -S $IFACE`).
    - Ensure no noisy neighbors on the VPS (if shared tenancy).
- **Application Crashes**:
    - Check `journalctl -u ops-node` and application-specific logs in `${APP_HOME_DIR}/app/logs`.
    - Look for OOM errors (`dmesg | grep -i kill`).
    - Ensure sufficient file descriptors (`ulimit -n`) and memlock limits for `opsnode` user if systemd is not used.
- **Configuration Issues**:
    - Double-check all paths in `ops-node.toml` and environment variables.
    - Verify API keys and endpoint URLs.

This guide provides a comprehensive starting point. Continuous monitoring and fine-tuning will be necessary to achieve optimal performance for your specific `ops-node` workload and trading strategies.
