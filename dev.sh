#!/usr/bin/env bash
set -euo pipefail
set +m

if [[ "$EUID" -eq 0 ]]; then
    printf 'Run ./dev.sh as your normal user, without sudo. Only Docker commands may use sudo.\n' >&2
    exit 1
fi

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

for command in docker cargo npm setsid; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'Missing required command: %s\n' "$command" >&2
        exit 1
    fi
done

docker_command=(docker)
if ! docker info >/dev/null 2>&1; then
    if ! command -v sudo >/dev/null 2>&1; then
        printf 'Docker is not accessible. Start Docker and provide direct access or install sudo.\n' >&2
        exit 1
    fi
    printf 'Docker needs elevated access. Authenticating with sudo...\n'
    if ! sudo -v; then
        printf 'Sudo authentication failed; development services were not started.\n' >&2
        exit 1
    fi
    docker_command=(sudo docker)
    if ! "${docker_command[@]}" info >/dev/null; then
        printf 'Docker is still unavailable. Check that the Docker daemon is running.\n' >&2
        exit 1
    fi
fi

compose() {
    "${docker_command[@]}" compose "$@"
}

compose version >/dev/null

processes=()

cleanup() {
    trap '' INT TERM
    printf '\nStopping development processes...\n'
    for pid in "${processes[@]}"; do
        kill -TERM -- "-$pid" 2>/dev/null || true
    done

    for attempt in {1..50}; do
        running=false
        for pid in "${processes[@]}"; do
            if kill -0 -- "-$pid" 2>/dev/null; then
                running=true
            fi
        done
        if [[ "$running" == false ]]; then
            break
        fi
        sleep 0.1
    done

    for pid in "${processes[@]}"; do
        kill -KILL -- "-$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    done
    printf 'Development processes stopped. MongoDB and its data are preserved.\n'
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

printf 'Starting MongoDB and waiting for its health check (up to 180 seconds)...\n'
if ! compose up -d --no-recreate --wait --wait-timeout 180 mongo; then
    printf 'MongoDB did not become healthy. Check docker compose logs mongo (using sudo if required).\n' >&2
    exit 1
fi

printf 'MongoDB is healthy: mongodb://localhost:27017\n'
printf 'Starting Rust backend: http://127.0.0.1:8080 (default; see BIND_ADDRESS)\n'
setsid cargo run -p fuel-ops &
processes+=("$!")

printf 'Starting React frontend: http://localhost:5173 (see Vite output for the actual URL)\n'
setsid npm --prefix web run dev &
processes+=("$!")

printf 'Application logs appear below. Press Ctrl+C to stop the backend and frontend.\n'
while true; do
    for pid in "${processes[@]}"; do
        if ! kill -0 "$pid" 2>/dev/null; then
            status=0
            wait "$pid" || status=$?
            printf 'A development process exited (status %s); stopping the other process.\n' "$status" >&2
            if [[ "$status" -eq 0 ]]; then
                status=1
            fi
            exit "$status"
        fi
    done
    sleep 1
done
