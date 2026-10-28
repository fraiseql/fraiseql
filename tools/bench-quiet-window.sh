#!/usr/bin/env bash
# Wait until this box is quiet enough to measure on, and say what it saw.
#
# Usage: tools/bench-quiet-window.sh [--max-load N] [--retries N] [--interval SECONDS]
#
# The box is busy while any of these holds:
#   - a CI job other than this one runs here: a Runner.Worker that is not an ancestor of
#     this process (forge is a CI runner as well as a dev box);
#   - a cargo or rustc runs anywhere on the box (a dev session, or another job's
#     container, whose processes the host sees);
#   - the 1-minute load average is over --max-load.
# A busy box is asked again every --interval seconds, up to --retries attempts in all.
#
# Prints one JSON record on stdout, quiet or not, for the benchmark to attach to its
# results:
#   {"quiet":true,"attempts":1,"waited_s":0,"load1":0.20,"load5":0.35,"load15":0.50,"busy":[]}
#
# Exit status: 0 quiet (measure), 3 still busy after every attempt (do not measure),
# 2 usage error or an unreadable load average.
#
# For tests: QW_PS_FILE replaces `ps -eo pid=,ppid=,comm=`; QW_LOADAVG_FILES is a
# space-separated list of loadavg files read one per attempt, the last one repeating;
# QW_SELF_PID replaces this process's pid; QW_SLEEP replaces `sleep`.
set -euo pipefail

max_load=1.5
retries=30
interval=60

while [ $# -gt 0 ]; do
    case "$1" in
        --max-load) max_load="${2:?--max-load needs a value}"; shift 2 ;;
        --retries) retries="${2:?--retries needs a value}"; shift 2 ;;
        --interval) interval="${2:?--interval needs a value}"; shift 2 ;;
        *) echo "bench-quiet-window: unknown argument: $1" >&2; exit 2 ;;
    esac
done
case "$retries$interval" in
    *[!0-9]*) echo "bench-quiet-window: --retries and --interval take whole numbers" >&2; exit 2 ;;
esac
[ "$retries" -ge 1 ] || { echo "bench-quiet-window: --retries must be at least 1" >&2; exit 2; }

self_pid="${QW_SELF_PID:-$$}"
read -r -a load_files <<< "${QW_LOADAVG_FILES:-/proc/loadavg}"
sleep_cmd="${QW_SLEEP:-sleep}"

process_listing() {
    if [ -n "${QW_PS_FILE:-}" ]; then cat "$QW_PS_FILE"; else ps -eo pid=,ppid=,comm=; fi
}

# busy_processes: "comm:pid" for every process that makes the box busy, one per line.
busy_processes() {
    process_listing | awk -v self="$self_pid" '
        { comm[$1] = $3; parent[$1] = $2 }
        END {
            for (p = self; p in parent && !(p in mine); p = parent[p]) mine[p] = 1
            for (p in comm) {
                c = comm[p]
                if (c == "cargo" || c == "rustc" || (c == "Runner.Worker" && !(p in mine)))
                    print c ":" p
            }
        }' | sort -t: -k2,2n
}

attempt=0
waited=0
while :; do
    attempt=$((attempt + 1))
    idx=$((attempt <= ${#load_files[@]} ? attempt - 1 : ${#load_files[@]} - 1))
    load_file="${load_files[$idx]}"
    if ! read -r load1 load5 load15 _ < "$load_file" 2>/dev/null; then
        echo "bench-quiet-window: cannot read the load average from $load_file" >&2
        exit 2
    fi

    busy=()
    while IFS= read -r line; do busy+=("$line"); done < <(busy_processes)
    if awk -v l="$load1" -v m="$max_load" 'BEGIN { exit !(l > m) }'; then
        busy+=("load1>$max_load")
    fi

    if [ ${#busy[@]} -eq 0 ] || [ "$attempt" -ge "$retries" ]; then
        break
    fi
    "$sleep_cmd" "$interval"
    waited=$((waited + interval))
done

quiet=$([ ${#busy[@]} -eq 0 ] && echo true || echo false)
busy_json=""
for b in ${busy[@]+"${busy[@]}"}; do busy_json="$busy_json${busy_json:+,}\"$b\""; done
printf '{"quiet":%s,"attempts":%d,"waited_s":%d,"load1":%s,"load5":%s,"load15":%s,"busy":[%s]}\n' \
    "$quiet" "$attempt" "$waited" "$load1" "$load5" "$load15" "$busy_json"
[ "$quiet" = true ] || exit 3
