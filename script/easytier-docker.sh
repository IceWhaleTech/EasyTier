#!/usr/bin/env bash

set -euo pipefail

SCRIPT_NAME="$(basename "${0:-install-docker.sh}")"
DEFAULT_IMAGE="easytier/easytier:v2.5.0"
DEFAULT_TCP_LISTENER="tcp://0.0.0.0:11010"
DEFAULT_UDP_LISTENER="udp://0.0.0.0:11010"
DEFAULT_WG_LISTENER="wg://0.0.0.0:11011"
DEFAULT_WS_LISTENER="ws://0.0.0.0:11011"
DEFAULT_WSS_LISTENER="wss://0.0.0.0:11012"
DEFAULT_FAKETCP_LISTENER="faketcp://0.0.0.0:11013"
DEFAULT_WS_80_LISTENER="ws://0.0.0.0:80"

HELP() {
  cat <<EOF
EasyTier Docker Installation Script

Usage:
  ${SCRIPT_NAME} <command> [options]

Commands:
  install      Pull the image and run an EasyTier container
  start        Start the existing EasyTier container
  stop         Stop the EasyTier container
  restart      Restart the EasyTier container
  status       Show EasyTier container status
  logs         Follow EasyTier container logs
  uninstall    Remove the EasyTier container
  help         Show this help message

Common options:
  --container-name NAME    Docker container name (default: easytier)
  --hostname NAME          Container hostname (default: easytier)
  --image IMAGE            Docker image (default: ${DEFAULT_IMAGE})
  --network-name NAME      Optional EasyTier network name
  --network-secret SECRET  Optional EasyTier network secret
  --peer URI               Add a peer, may be specified multiple times
  --ipv4 CIDR              Set a static EasyTier IPv4 address instead of DHCP
  --dhcp                   Request an IPv4 address by DHCP
  --rpc-portal ADDR        Set the RPC portal address
  --no-tun                 Run EasyTier with --no-tun
  --extra-arg ARG          Append an extra EasyTier argument, may be repeated
  --tz TZ                  Set TZ for the container (default: current TZ or Asia/Shanghai)
  --force                  Recreate the container on install if it already exists
  --skip-docker-install    Fail instead of auto-installing Docker when docker is missing
  --dry-run                Print actions without changing the system

Notes:
  If --network-name/--network-secret are both omitted, the script explicitly passes
  empty values so the node stays out of the built-in "default" network and behaves
  more like a pure relay/shared node bootstrap.

Examples:
  curl -fsSL "https://raw.githubusercontent.com/EasyTier/EasyTier/main/script/install-docker.sh" | \\
    sudo bash -s -- install

  curl -fsSL "https://raw.githubusercontent.com/EasyTier/EasyTier/main/script/install-docker.sh" | \\
    sudo bash -s -- install --network-name mynet --network-secret mysecret --peer udp://YOUR_SERVER_PUBLIC_IP:11010 --dhcp

  curl -fsSL "https://raw.githubusercontent.com/EasyTier/EasyTier/main/script/install-docker.sh" | \\
    sudo bash -s -- logs

EOF
}

info() {
  printf '[INFO] %s\n' "$*"
}

warn() {
  printf '[WARN] %s\n' "$*" >&2
}

die() {
  printf '[ERROR] %s\n' "$*" >&2
  exit 1
}

set_defaults() {
  IMAGE="${DEFAULT_IMAGE}"
  CONTAINER_NAME="easytier"
  HOST_NAME="easytier"
  TZ_VALUE="${TZ:-Asia/Shanghai}"
  NETWORK_NAME=""
  NETWORK_SECRET=""
  IPV4=""
  DHCP="false"
  RPC_PORTAL=""
  NO_TUN="true"
  AUTO_INSTALL_DOCKER="true"
  FORCE="false"
  DRY_RUN="false"
  PEERS=()
  EXTRA_ARGS=()
}

run_cmd() {
  if [[ "${DRY_RUN}" == "true" ]]; then
    printf '+'
    for arg in "$@"; do
      printf ' %q' "${arg}"
    done
    printf '\n'
    return 0
  fi

  "$@"
}

ensure_linux() {
  [[ "$(uname -s)" == "Linux" ]] || die "This script only supports Linux hosts."
}

ensure_root() {
  [[ "$(id -u)" == "0" ]] || die "Please run this script as root, for example with sudo."
}

wait_for_docker() {
  local i
  for i in $(seq 1 15); do
    if docker info >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  return 1
}

start_docker_service() {
  if command -v systemctl >/dev/null 2>&1; then
    run_cmd systemctl enable --now docker || run_cmd systemctl start docker || true
  elif command -v service >/dev/null 2>&1; then
    run_cmd service docker start || true
  fi
}

install_docker() {
  info "Docker is not installed. Installing Docker with the official convenience script."
  run_cmd bash -lc 'curl -fsSL https://get.docker.com | sh'
  start_docker_service

  if [[ "${DRY_RUN}" == "true" ]]; then
    return 0
  fi

  wait_for_docker || die "Docker was installed but the daemon is not ready. Please start docker manually and rerun the script."
}

ensure_docker() {
  if command -v docker >/dev/null 2>&1; then
    if ! docker info >/dev/null 2>&1; then
      start_docker_service
      if [[ "${DRY_RUN}" != "true" ]]; then
        wait_for_docker || die "Docker is installed but the daemon is not reachable."
      fi
    fi
    return 0
  fi

  [[ "${AUTO_INSTALL_DOCKER}" == "true" ]] || die "docker command not found. Install Docker first or remove --skip-docker-install."
  install_docker
}

ensure_tun_device() {
  [[ "${NO_TUN}" == "true" ]] && return 0

  if [[ -e /dev/net/tun ]]; then
    return 0
  fi

  if command -v modprobe >/dev/null 2>&1; then
    run_cmd modprobe tun || true
  fi

  [[ -e /dev/net/tun ]] || die "/dev/net/tun is unavailable. Enable TUN on the host or reinstall with --no-tun."
}

container_exists() {
  docker container inspect "${CONTAINER_NAME}" >/dev/null 2>&1
}

container_running() {
  [[ "$(docker inspect -f '{{.State.Running}}' "${CONTAINER_NAME}" 2>/dev/null || true)" == "true" ]]
}

pull_image() {
  info "Pulling image ${IMAGE}"
  run_cmd docker pull "${IMAGE}"
}

should_add_default_listeners() {
  if [[ "${#EXTRA_ARGS[@]}" -eq 0 ]]; then
    return 0
  fi

  local arg
  for arg in "${EXTRA_ARGS[@]}"; do
    case "${arg}" in
      --listeners|-l|--no-listener)
        return 1
        ;;
    esac
  done

  return 0
}

build_easytier_args() {
  EASYTIER_ARGS=()

  if [[ -n "${IPV4}" ]]; then
    EASYTIER_ARGS+=("--ipv4" "${IPV4}")
  elif [[ "${DHCP}" == "true" ]]; then
    EASYTIER_ARGS+=("--dhcp")
  fi

  # Always pass network identity explicitly. Empty values avoid falling back to
  # EasyTier's built-in default network identity ("default"/"").
  EASYTIER_ARGS+=("--network-name" "${NETWORK_NAME}")
  EASYTIER_ARGS+=("--network-secret" "${NETWORK_SECRET}")

  if [[ -n "${RPC_PORTAL}" ]]; then
    EASYTIER_ARGS+=("--rpc-portal" "${RPC_PORTAL}")
  fi

  if [[ "${NO_TUN}" == "true" ]]; then
    EASYTIER_ARGS+=("--no-tun")
  fi

  if should_add_default_listeners; then
    EASYTIER_ARGS+=(
      "--listeners" "${DEFAULT_TCP_LISTENER}"
      "--listeners" "${DEFAULT_UDP_LISTENER}"
      "--listeners" "${DEFAULT_WG_LISTENER}"
      "--listeners" "${DEFAULT_WS_LISTENER}"
      "--listeners" "${DEFAULT_WSS_LISTENER}"
      "--listeners" "${DEFAULT_FAKETCP_LISTENER}"
      "--listeners" "${DEFAULT_WS_80_LISTENER}"
    )
  fi

  if [[ "${#PEERS[@]}" -gt 0 ]]; then
    local peer
    for peer in "${PEERS[@]}"; do
      EASYTIER_ARGS+=("--peer" "${peer}")
    done
  fi

  if [[ "${#EXTRA_ARGS[@]}" -gt 0 ]]; then
    local arg
    for arg in "${EXTRA_ARGS[@]}"; do
      EASYTIER_ARGS+=("${arg}")
    done
  fi
}

build_docker_run_args() {
  build_easytier_args

  DOCKER_RUN_ARGS=(
    docker run -d
    --name "${CONTAINER_NAME}"
    --hostname "${HOST_NAME}"
    --restart unless-stopped
    --network host
    -e "TZ=${TZ_VALUE}"
  )

  if [[ "${NO_TUN}" != "true" ]]; then
    ensure_tun_device
    DOCKER_RUN_ARGS+=(--cap-add NET_ADMIN --cap-add NET_RAW --device /dev/net/tun:/dev/net/tun)
  fi

  DOCKER_RUN_ARGS+=("${IMAGE}")
  DOCKER_RUN_ARGS+=("${EASYTIER_ARGS[@]}")
}

remove_container() {
  if container_exists; then
    info "Removing container ${CONTAINER_NAME}"
    run_cmd docker rm -f "${CONTAINER_NAME}"
  fi
}

validate_install_config() {
  if [[ -n "${NETWORK_NAME}" && -z "${NETWORK_SECRET}" ]]; then
    die "--network-secret must be provided when --network-name is set."
  fi

  if [[ -z "${NETWORK_NAME}" && -n "${NETWORK_SECRET}" ]]; then
    die "--network-name must be provided when --network-secret is set."
  fi
}

print_success() {
  cat <<EOF
EasyTier Docker container is ready.

Container name : ${CONTAINER_NAME}
Image          : ${IMAGE}

Useful commands:
  docker logs -f ${CONTAINER_NAME}
  docker exec -it ${CONTAINER_NAME} easytier-cli peer
  docker exec -it ${CONTAINER_NAME} easytier-cli node
EOF
}

do_install() {
  validate_install_config
  ensure_docker

  if container_exists; then
    [[ "${FORCE}" == "true" ]] || die "Container ${CONTAINER_NAME} already exists. Re-run with --force to recreate."
    remove_container
  fi

  pull_image
  build_docker_run_args
  info "Starting EasyTier container ${CONTAINER_NAME}"
  run_cmd "${DOCKER_RUN_ARGS[@]}"
  print_success
}

do_start() {
  ensure_docker
  container_exists || die "Container ${CONTAINER_NAME} does not exist. Re-run install."
  if container_running; then
    info "Container ${CONTAINER_NAME} is already running."
    return 0
  fi
  run_cmd docker start "${CONTAINER_NAME}"
}

do_stop() {
  ensure_docker
  container_exists || die "Container ${CONTAINER_NAME} does not exist."
  if container_running; then
    run_cmd docker stop "${CONTAINER_NAME}"
  else
    info "Container ${CONTAINER_NAME} is already stopped."
  fi
}

do_restart() {
  ensure_docker
  container_exists || die "Container ${CONTAINER_NAME} does not exist."
  run_cmd docker restart "${CONTAINER_NAME}"
}

do_status() {
  ensure_docker
  container_exists || die "Container ${CONTAINER_NAME} does not exist."
  run_cmd docker ps -a --filter "name=^/${CONTAINER_NAME}$" --format 'table {{.Names}}\t{{.Status}}\t{{.Image}}'
}

do_logs() {
  ensure_docker
  container_exists || die "Container ${CONTAINER_NAME} does not exist."
  run_cmd docker logs -f "${CONTAINER_NAME}"
}

do_uninstall() {
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    remove_container
  else
    warn "Docker is not available, skipping container removal."
  fi
}

parse_args() {
  local extra_args_overridden="false"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --container-name)
        [[ $# -ge 2 ]] || die "--container-name requires a value"
        CONTAINER_NAME="$2"
        shift 2
        ;;
      --hostname)
        [[ $# -ge 2 ]] || die "--hostname requires a value"
        HOST_NAME="$2"
        shift 2
        ;;
      --image)
        [[ $# -ge 2 ]] || die "--image requires a value"
        IMAGE="$2"
        shift 2
        ;;
      --network-name)
        [[ $# -ge 2 ]] || die "--network-name requires a value"
        NETWORK_NAME="$2"
        shift 2
        ;;
      --network-secret)
        [[ $# -ge 2 ]] || die "--network-secret requires a value"
        NETWORK_SECRET="$2"
        shift 2
        ;;
      --peer)
        [[ $# -ge 2 ]] || die "--peer requires a value"
        PEERS+=("$2")
        shift 2
        ;;
      --ipv4)
        [[ $# -ge 2 ]] || die "--ipv4 requires a value"
        IPV4="$2"
        DHCP="false"
        shift 2
        ;;
      --dhcp)
        DHCP="true"
        IPV4=""
        shift
        ;;
      --rpc-portal)
        [[ $# -ge 2 ]] || die "--rpc-portal requires a value"
        RPC_PORTAL="$2"
        shift 2
        ;;
      --no-tun)
        NO_TUN="true"
        shift
        ;;
      --extra-arg)
        [[ $# -ge 2 ]] || die "--extra-arg requires a value"
        if [[ "${extra_args_overridden}" != "true" ]]; then
          EXTRA_ARGS=()
          extra_args_overridden="true"
        fi
        EXTRA_ARGS+=("$2")
        shift 2
        ;;
      --tz)
        [[ $# -ge 2 ]] || die "--tz requires a value"
        TZ_VALUE="$2"
        shift 2
        ;;
      --force)
        FORCE="true"
        shift
        ;;
      --skip-docker-install)
        AUTO_INSTALL_DOCKER="false"
        shift
        ;;
      --dry-run)
        DRY_RUN="true"
        shift
        ;;
      -h|--help)
        HELP
        exit 0
        ;;
      *)
        die "Unknown option: $1"
        ;;
    esac
  done
}

main() {
  local command="${1:-help}"
  shift || true

  set_defaults

  case "${command}" in
    help)
      HELP
      ;;
    install)
      parse_args "$@"
      ensure_linux
      ensure_root
      do_install
      ;;
    start)
      parse_args "$@"
      ensure_linux
      ensure_root
      do_start
      ;;
    stop)
      parse_args "$@"
      ensure_linux
      ensure_root
      do_stop
      ;;
    restart)
      parse_args "$@"
      ensure_linux
      ensure_root
      do_restart
      ;;
    status)
      parse_args "$@"
      ensure_linux
      ensure_root
      do_status
      ;;
    logs)
      parse_args "$@"
      ensure_linux
      ensure_root
      do_logs
      ;;
    uninstall)
      parse_args "$@"
      ensure_linux
      ensure_root
      do_uninstall
      ;;
    *)
      die "Unknown command: ${command}. Use '${SCRIPT_NAME} help' for usage."
      ;;
  esac
}

main "$@"
