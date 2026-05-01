REPO=/home/a/vibe-coded-os
LOG=$REPO/logs/hermes-loop.log
mkdir -p "$REPO/logs"

echo "[$(date '+%Y-%m-%d %H:%M:%S')] Hermes loop starting" | tee -a "$LOG"

while true; do
    cd "$REPO"
    git fetch origin main 2>/dev/null || true

    # Check new issues
    if command -v gh >/dev/null 2>&1; then
        NEW_ISS=$(gh issue list --repo theworkingman-beep/vibe-coded-os --state open --json number,title --jq '.[] | "\(.number): \(.title)"' 2>/dev/null)
        if [ -n "$NEW_ISS" ]; then
            echo "[$(date '+%Y-%m-%d %H:%M:%S')] ISSUES: $NEW_ISS" | tee -a "$LOG"
        fi
    fi

    # Quick build
    if ! TARGET_ARCH=x86_64 ./scripts/build.sh >/tmp/hermes-build.log 2>&1; then
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] BUILD FAILURE" | tee -a "$LOG"
        tail -5 /tmp/hermes-build.log | tee -a "$LOG"
    else
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] build OK" | tee -a "$LOG"
    fi

    # Commit/push
    if [ -n "$(git status --porcelain)" ]; then
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] committing..." | tee -a "$LOG"
        git add -A
        git commit -m "autocommit: $(date -Iseconds)" --no-edit || true
        git push origin main 2>/dev/null || true
    fi

    # Self-prompt
    echo "prompt: continue" > /home/a/PROMPT.txt
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] loop end" | tee -a "$LOG"
    sleep 300
done
