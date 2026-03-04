#!/usr/bin/env node
// Claude Code Statusline - GSD Edition
// Shows: model | current task | directory | context usage

const fs = require('fs');
const path = require('path');
const os = require('os');
// const { execSync } = require('child_process');

const BLUE = "\x1b[34m";
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";
const CYAN = "\x1b[36m";
const RESET = "\x1b[0m";

const USAGE_API_URL = "https://api.anthropic.com/api/oauth/usage";
const USAGE_THRESHOLD_HIGH = 80;
const USAGE_THRESHOLD_MEDIUM = 50;
const CREDENTIALS_PATH = path.join(os.homedir(), ".claude", ".credentials.json");

/**
 * Read access token from credentials file.
 */
function getAccessToken() {
    try {
        const data = fs.readFileSync(CREDENTIALS_PATH, 'utf8');
        const creds = JSON.parse(data);
        return creds?.claudeAiOauth?.accessToken || null;
    } catch (error) {
        return null;
    }
}

/**
 * Fetch usage data from Anthropic API.
 */
async function fetchUsage(accessToken) {
    try {
        const response = await fetch(USAGE_API_URL, {
            method: 'GET',
            headers: {
                "Authorization": `Bearer ${accessToken}`,
                "Content-Type": "application/json",
                "anthropic-beta": "oauth-2025-04-20",
            },
            signal: AbortSignal.timeout(5000) // 5 second timeout
        });

        if (!response.ok) return null;
        return await response.json();
    } catch (error) {
        console.error("Failed to fetch")
        return null;
    }
}

function getUsageColor(percentage) {
    if (percentage >= USAGE_THRESHOLD_HIGH) return RED;
    if (percentage >= USAGE_THRESHOLD_MEDIUM) return YELLOW;
    return GREEN;
}

const formatUsage = (val, pre = "", post = "%", col = undefined, dp = 0) => {
    // Convert string from split() to a number, or keep as NaN/undefined
    num = parseFloat(val); 
    
    if (isNaN(num)) return `${RED}NA${RESET}`

    if (col == undefined) {
      col = getUsageColor(num);
    }
    
    return `${col}${pre}${num.toFixed(dp)}${post}${RESET}`;
};

function formatDuration(isoString) {
    if (!isoString) return "";

    const target = new Date(isoString);
    const now = new Date();
    const diffMs = target - now;

    if (isNaN(target.getTime()) || diffMs <= 0) {
        return " now"; 
    }

    const diffMins = Math.round(diffMs / 60000);
    const diffHours = Math.floor(diffMins / 60);
    const remainingMins = diffMins % 60;

    if (diffHours > 0) {
        return ` ${diffHours}h ${remainingMins}m`;
    }
    return ` ${diffMins}m`;
}

// Read JSON from stdin
let input = '';
// Timeout guard: if stdin doesn't close within 3s (e.g. pipe issues on
// Windows/Git Bash), exit silently instead of hanging. See #775.
const stdinTimeout = setTimeout(() => process.exit(0), 3000);
process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => input += chunk);
process.stdin.on('end', async () => {
  clearTimeout(stdinTimeout);
  try {
    const data = JSON.parse(input);
    const model = data.model?.display_name || 'Claude';
    const dir = data.workspace?.current_dir || process.cwd();
    const session = data.session_id || '';
    const remaining = data.context_window?.remaining_percentage;
    const cost = data.cost?.total_cost_usd ?? 0.0;
    const tok_out = data.context_window?.total_output_tokens ?? 0.0;
    const tok_in = data.context_window?.total_input_tokens ?? 0.0;

    // Usage limits
    const homeDir = os.homedir();
    const CACHE_MAX_AGE = 30; // seconds
    const CACHE_FILE = path.join(homeDir, '.claude', 'cache', 'gsd-usage.txt');

    const cacheIsStale = () => {
        if (!fs.existsSync(CACHE_FILE)) return true;
        return (Date.now() / 1000) - fs.statSync(CACHE_FILE).mtimeMs / 1000 > CACHE_MAX_AGE;
    };

    if (cacheIsStale()) {
        try {
            const token = getAccessToken();
            if (!token) throw new Error("No token found");
            const usageData = await fetchUsage(token);
            // console.error(usageData)

            fs.writeFileSync(CACHE_FILE, `${usageData.five_hour?.utilization ?? 0}|${usageData.five_hour?.resets_at ?? ""}|${usageData.seven_day?.utilization ?? 0}|${usageData.seven_day?.resets_at ?? ""}|${(usageData.extra_usage?.used_credits ?? 0) / 100.0}`);
        } catch {
            fs.writeFileSync(CACHE_FILE, '||||');
        }
    }

    const [u_session, u_session_reset, u_week, u_week_reset, u_extra] = fs.readFileSync(CACHE_FILE, 'utf8').trim().split('|');

    // Context window display (shows USED percentage scaled to usable context)
    // Claude Code reserves ~16.5% for autocompact buffer, so usable context
    // is 83.5% of the total window. We normalize to show 100% at that point.
    const AUTO_COMPACT_BUFFER_PCT = 16.5;
    let ctx = '';
    if (remaining != null) {
      // Normalize: subtract buffer from remaining, scale to usable range
      const usableRemaining = Math.max(0, ((remaining - AUTO_COMPACT_BUFFER_PCT) / (100 - AUTO_COMPACT_BUFFER_PCT)) * 100);
      const used = Math.max(0, Math.min(100, Math.round(100 - usableRemaining)));

      // Write context metrics to bridge file for the context-monitor PostToolUse hook.
      // The monitor reads this file to inject agent-facing warnings when context is low.
      if (session) {
        try {
          const bridgePath = path.join(os.tmpdir(), `claude-ctx-${session}.json`);
          const bridgeData = JSON.stringify({
            session_id: session,
            remaining_percentage: remaining,
            used_pct: used,
            timestamp: Math.floor(Date.now() / 1000)
          });
          fs.writeFileSync(bridgePath, bridgeData);
        } catch (e) {
          // Silent fail -- bridge is best-effort, don't break statusline
        }
      }

      // Build progress bar (10 segments)
      const filled = Math.floor(used / 10);
      const bar = '█'.repeat(filled) + '░'.repeat(10 - filled);

      // Color based on usable context thresholds
      if (used < 50) {
        ctx = ` \x1b[32m${bar} ${used}%\x1b[0m`;
      } else if (used < 65) {
        ctx = ` \x1b[33m${bar} ${used}%\x1b[0m`;
      } else if (used < 80) {
        ctx = ` \x1b[38;5;208m${bar} ${used}%\x1b[0m`;
      } else {
        ctx = ` \x1b[5;31m💀 ${bar} ${used}%\x1b[0m`;
      }
    }

    // Current task from todos
    let task = '';
    // Respect CLAUDE_CONFIG_DIR for custom config directory setups (#870)
    const claudeDir = process.env.CLAUDE_CONFIG_DIR || path.join(homeDir, '.claude');
    const todosDir = path.join(claudeDir, 'todos');
    if (session && fs.existsSync(todosDir)) {
      try {
        const files = fs.readdirSync(todosDir)
          .filter(f => f.startsWith(session) && f.includes('-agent-') && f.endsWith('.json'))
          .map(f => ({ name: f, mtime: fs.statSync(path.join(todosDir, f)).mtime }))
          .sort((a, b) => b.mtime - a.mtime);

        if (files.length > 0) {
          try {
            const todos = JSON.parse(fs.readFileSync(path.join(todosDir, files[0].name), 'utf8'));
            const inProgress = todos.find(t => t.status === 'in_progress');
            if (inProgress) task = inProgress.activeForm || '';
          } catch (e) {}
        }
      } catch (e) {
        // Silently fail on file system errors - don't break statusline
      }
    }

    // GSD update available?
    let gsdUpdate = '';
    const cacheFile = path.join(claudeDir, 'cache', 'gsd-update-check.json');
    if (fs.existsSync(cacheFile)) {
      try {
        const cache = JSON.parse(fs.readFileSync(cacheFile, 'utf8'));
        if (cache.update_available) {
          gsdUpdate = '\x1b[33m⬆ /gsd:update\x1b[0m │ ';
        }
      } catch (e) {}
    }

    // Output
    const dirname = path.basename(dir);
    if (task) {
      process.stdout.write(`${gsdUpdate}\x1b[2m${model}\x1b[0m │ \x1b[1m${task}\x1b[0m │ \x1b[2m${dirname}\x1b[0m${ctx}`);
    } else {
      process.stdout.write(`${gsdUpdate}\x1b[2m${model}\x1b[0m │ \x1b[2m${dirname}\x1b[0m${ctx}`);
    }

    if (u_session.length > 0) {
      process.stdout.write(`\n${formatUsage(u_session)}${formatDuration(u_session_reset)} │ ${formatUsage(u_week)}${formatDuration(u_week_reset)} | extra:${formatUsage(u_extra, "$", "", YELLOW, 2)} equiv:${formatUsage(cost, "$", "", YELLOW, 2)}| tokens: ${tok_in}:${tok_out}`);
    }
  } catch (e) {
    // Silent fail - don't break statusline on parse errors
  }
});
