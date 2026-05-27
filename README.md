<div align="center">

# ◈ Synapse-Overlord

**Local-first AI project builder and autonomous architecture assistant — powered by Hermes Agent.**

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-orange?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/Axum-0.8-blue?logo=rust)](https://github.com/tokio-rs/axum)
[![Hermes Agent](https://img.shields.io/badge/Powered%20by-Hermes%20Agent-8b5cf6?logo=anthropic&logoColor=white)](https://nousresearch.com/hermes/)
[![Groq](https://img.shields.io/badge/Inference-Groq%20API-f97316?logo=groq)](https://console.groq.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-22c55e.svg)](LICENSE)
[![Status](https://img.shields.io/badge/Status-Challenge%20Ready-22c55e)](https://github.com/fokrulanthro16-eng/synapse-overlord)

<br>

> Synapse-Overlord is a **real working product**, not a demo.  
> The Hermes Architect agent reads your actual source files, reasons about them, and returns structured engineering analysis — every tool call is logged and visible in the dashboard.

<br>

![Demo placeholder — see Setup section to run locally](https://placehold.co/900x480/0d0d1f/00d4ff?text=Synapse-Overlord+%E2%97%88+Hermes+Architect+Dashboard&font=courier)

*Dashboard · Hermes Architect panel · Tool-use trace · Engineering suggestions*

</div>

---

## Table of Contents

- [What is Synapse-Overlord?](#what-is-synapse-overlord)
- [Built with Hermes Agent](#built-with-hermes-agent)
- [How Hermes Agent Works](#how-hermes-agent-works)
- [Architecture Diagram](#architecture-diagram)
- [Feature List](#feature-list)
- [Local Setup](#local-setup)
- [Testing Run Agent](#testing-run-agent)
- [API Reference](#api-reference)
- [Project Structure](#project-structure)
- [Tech Stack](#tech-stack)
- [Hermes Agent Challenge](#hermes-agent-challenge)

---

## What is Synapse-Overlord?

Synapse-Overlord is a **local-first AI system** with two main capabilities:

**1 · Hermes Architect** — an autonomous code analysis agent that uses structured tool-use to explore your project, reason about its architecture, and return concrete engineering improvement suggestions.

**2 · AI Project Builder** — generate complete static web projects from a single natural-language command, preview them live in the browser, enhance them with follow-up instructions, and download as ZIP.

Everything runs locally. The only external dependency is the Groq API for LLM inference (offline fallback available).

---

## Built with Hermes Agent

<div align="center">

```
⚡ BUILT WITH HERMES AGENT
```

</div>

The **Hermes Architect** is the core of Synapse-Overlord. It is a real agentic system:

- Uses **structured tool-use** — the model calls tools, gets results, reasons, calls more tools
- Every tool call is **logged, validated, and returned** in the API response
- The agent **reads actual source files** from your project — not mock data
- Suggestions are **extracted as structured data** (`suggestions[]`) from the model's output
- A **human-in-the-loop patch system** lets the agent propose code changes that require explicit approval before anything touches disk

The `◆ Run Agent` button in the dashboard launches the full agentic loop against your local codebase.

---

## How Hermes Agent Works

```
┌─────────────────────────────────────────────────────────────────┐
│                    HERMES ARCHITECT LOOP                        │
│                                                                 │
│  User clicks ◆ Run Agent                                        │
│         │                                                       │
│         ▼                                                       │
│  POST /api/architect/analyze                                    │
│    { goal: "...", max_tool_calls: 7 }                          │
│         │                                                       │
│         ▼                                                       │
│  1. Map project (RAG file scanner)                              │
│     → discovers all source files, roles, sizes                  │
│         │                                                       │
│         ▼                                                       │
│  2. Build Hermes conversation                                   │
│     → system prompt with tool schemas injected                  │
│     → project file map included as context                      │
│         │                                                       │
│         ▼                                                       │
│  3. Agentic loop (up to 7 iterations)                          │
│     │                                                           │
│     ├──▶ LLM call ──▶ parse <tool_call> tags                   │
│     │                        │                                  │
│     │         ┌──────────────┴──────────────┐                  │
│     │         │              │              │                   │
│     │    file_list      file_read     file_search               │
│     │    (ls dir)     (read file)   (grep pattern)              │
│     │         │              │              │                   │
│     │         └──────────────┴──────────────┘                  │
│     │                        │                                  │
│     │              push result into conversation                 │
│     │                        │                                  │
│     └──────────── repeat until no tool calls ──────────────────┘
│         │                                                       │
│         ▼                                                       │
│  4. Synthesize final response                                   │
│     → extract summary (first 2 paragraphs)                      │
│     → extract suggestions (bullet points)                       │
│         │                                                       │
│         ▼                                                       │
│  JSON response:                                                 │
│    { status, summary, suggestions[], tool_calls[],              │
│      files_analyzed, timing_ms, model, backend }               │
└─────────────────────────────────────────────────────────────────┘
```

### Tool definitions

| Tool | Input | Output | Safety |
|---|---|---|---|
| `file_list` | `path` (relative) | Directory listing, hidden + build dirs excluded | Read-only |
| `file_read` | `path`, `max_lines` (≤500) | Numbered source lines | Read-only |
| `file_search` | `pattern`, `path` | Matching lines with file + line number | Read-only |
| `propose_patch` | `path`, `description`, `new_content` | Patch ID stored in memory | No disk write |

All paths are validated with `canonicalize() + starts_with(project_root)` — path traversal is impossible. Absolute paths are rejected at the input layer.

### Phase 3 — Safe patch proposal

```
POST /api/architect/propose
  → agent proposes patches (stored in memory, 10-min TTL)
  → returns patch IDs for human review

POST /api/architect/apply/{patch_id}
  → human approves ONE patch
  → backup written to .synapse_backups/
  → patch written to disk
  → cargo check runs automatically
  → result returned

POST /api/architect/reject/{patch_id}
  → patch discarded, nothing written
```

---

## Architecture Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                      SYNAPSE-OVERLORD                            │
│                                                                  │
│  ┌─────────────┐    ┌───────────────────────────────────────┐   │
│  │   Browser   │    │           Axum Web Server             │   │
│  │  Dashboard  │◀──▶│           (127.0.0.1:3000)            │   │
│  │  (HTML/JS)  │    │                                       │   │
│  └─────────────┘    │  Routes                               │   │
│                     │  ├── GET  /                           │   │
│                     │  ├── GET  /api/health                 │   │
│                     │  ├── POST /api/architect/analyze  ◀───┼── │── Hermes Agent
│                     │  ├── POST /api/architect/propose      │   │
│                     │  ├── POST /api/architect/apply/{id}   │   │
│                     │  ├── POST /api/command                │   │
│                     │  ├── GET  /api/generated-projects     │   │
│                     │  └── ...                              │   │
│                     └───────────────────────────────────────┘   │
│                                    │                             │
│            ┌───────────────────────┼──────────────────┐         │
│            │                       │                  │         │
│     ┌──────▼──────┐    ┌───────────▼──────┐   ┌──────▼──────┐  │
│     │   Hermes    │    │    Project       │   │   System    │  │
│     │  Architect  │    │    Builder       │   │   Monitor   │  │
│     │             │    │    + Enhancer    │   │  (CPU/RAM)  │  │
│     │ ┌─────────┐ │    └──────────────────┘   └─────────────┘  │
│     │ │Tool Loop│ │                                             │
│     │ │file_list│ │    ┌──────────────────┐   ┌─────────────┐  │
│     │ │file_read│ │    │  RAG File Mapper │   │   SQLite    │  │
│     │ │file_srch│ │    │  (project scan)  │   │  Database   │  │
│     │ └────┬────┘ │    └──────────────────┘   └─────────────┘  │
│     └──────┼──────┘                                             │
│            │                                                     │
│            ▼                                                     │
│     ┌─────────────┐    ┌──────────────────┐                     │
│     │  Groq API   │    │  Ollama (local)  │                     │
│     │  (cloud)    │    │  (offline mode)  │                     │
│     └─────────────┘    └──────────────────┘                     │
└──────────────────────────────────────────────────────────────────┘
```

---

## Feature List

### Hermes Architect Agent
- ✅ Real agentic tool-use loop (`file_list` → `file_read` → `file_search`)
- ✅ Structured JSON response with `status`, `summary`, `suggestions[]`, `tool_calls[]`
- ✅ Loading states: **Scanning → Reading Files → Analyzing → Ready**
- ✅ Tool call chips in dashboard (✓/✗ per tool)
- ✅ Meta bar: files analyzed, tool call count, timing, model/backend
- ✅ Suggestions extracted as structured array from model output
- ✅ Full analysis text rendered in Hermes Architect panel
- ✅ Safe patch proposal workflow (propose → human review → apply → cargo check)

### Project Builder
- ✅ Natural-language project generation (`build project <idea>`)
- ✅ 12+ project templates (medical, portfolio, ecommerce, restaurant, quiz, expense tracker…)
- ✅ Live in-browser project preview
- ✅ Download as ZIP
- ✅ README viewer
- ✅ Project enhancer: `improve project` / `add feature`
- ✅ Automatic backup before any project edit

### Dashboard
- ✅ Real-time CPU + RAM metrics (1-second polling)
- ✅ Agent status indicator (Idle / Running)
- ✅ Execution log with per-tool trace
- ✅ Settings: profile, AI models, API keys, IDE connector
- ✅ Database explorer: SQLite tables, schema viewer, SQL query runner
- ✅ Connection manager: SQLite, PostgreSQL, MySQL, MongoDB, Supabase, Neon…
- ✅ Workspace / saved project switcher
- ✅ Generated projects gallery with search + type filter
- ✅ "Built with Hermes Agent" badge in sidebar

### Safety
- ✅ No auto-apply of any code change
- ✅ No shell execution except `cargo check`
- ✅ Path traversal blocked at input layer
- ✅ Patch proposals expire after 10 minutes
- ✅ Backups created before every file write
- ✅ No file deletions

---

## Local Setup

### Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | stable (2024 edition) | [rustup.rs](https://rustup.rs/) |
| Groq API key | any | [console.groq.com](https://console.groq.com/) (free tier available) |

### Steps

```bash
# 1. Clone
git clone https://github.com/fokrulanthro16-eng/synapse-overlord.git
cd synapse-overlord

# 2. Set up environment
cp .env.example .env
# Edit .env and paste your Groq API key:
#   GROQ_API_KEY=gsk_your_key_here

# 3. Build and run
cargo run -- web
```

Open **http://127.0.0.1:3000** in your browser.

### Alternative run modes

```bash
cargo run -- web      # web dashboard  (same as --web, serve, -w)
cargo run             # terminal TUI mode
```

### Offline / no API key

The system works without a key — project generation, preview, ZIP download, and the dashboard all function offline. Only the Hermes Architect analysis and AI model calls require `GROQ_API_KEY`.

---

## Testing Run Agent

### In the browser

1. Open `http://127.0.0.1:3000`
2. Make sure `GROQ_API_KEY` is in `.env`
3. Click **◆ Run Agent** in the left sidebar
4. Watch the status pill cycle through:
   - `◌ SCANNING PROJECT`
   - `◌ READING FILES`
   - `◌ ANALYZING`
   - `✓ READY`
5. **HERMES ARCHITECT panel** (left) shows:
   - Meta bar: files scanned, tool calls used, timing, backend
   - Tool call chips: each `file_list` / `file_read` / `file_search` call with ✓/✗
   - **SUGGESTIONS** — structured bullet points
   - **FULL ANALYSIS** — complete model output
6. **Execution Log** (right) shows the detailed per-tool trace

### Via curl

```bash
# Basic analysis (adjust max_tool_calls for depth)
curl -s -X POST http://127.0.0.1:3000/api/architect/analyze \
  -H "Content-Type: application/json" \
  -d '{"goal":"Analyze this Rust project and suggest improvements","max_tool_calls":7}' \
  | jq '{status, files_analyzed, timing_ms, suggestions}'
```

**Expected response shape:**

```json
{
  "status": "success",
  "success": true,
  "summary": "Synapse-Overlord is a Rust/Axum web application...",
  "analysis": "...",
  "suggestions": [
    "Add integration tests for the Hermes agent loop",
    "Extract the dashboard HTML into a separate template file",
    "Add rate limiting to the /api/architect/analyze endpoint"
  ],
  "tool_calls": [
    { "name": "file_list",   "success": true, "result_preview": "src/\n  main.rs\n  agent/..." },
    { "name": "file_read",   "success": true, "result_preview": "1  mod agent;\n2  mod builder;..." },
    { "name": "file_search", "success": true, "result_preview": "src/routes/hermes.rs:118: pub async fn..." }
  ],
  "files_analyzed": 62,
  "files_mapped": 62,
  "model": "llama-3.3-70b-versatile",
  "backend": "groq",
  "timing_ms": 3241,
  "duration_ms": 3241
}
```

### Propose a patch (Phase 3)

```bash
# Ask the agent to propose a code improvement
curl -s -X POST http://127.0.0.1:3000/api/architect/propose \
  -H "Content-Type: application/json" \
  -d '{"goal":"Add a health check comment to src/main.rs","max_tool_calls":3}' \
  | jq '{success, patches: [.patches[].patch_id]}'

# Approve a patch (replace PATCH_ID with the returned id)
curl -s -X POST http://127.0.0.1:3000/api/architect/apply/PATCH_ID

# Or reject it
curl -s -X POST http://127.0.0.1:3000/api/architect/reject/PATCH_ID

# List all pending patches
curl -s http://127.0.0.1:3000/api/architect/patches | jq '.patches[].patch_id'
```

---

## API Reference

### Hermes Architect

| Method | Route | Description |
|---|---|---|
| `POST` | `/api/architect/analyze` | Run analysis — reads files, returns structured JSON. **Never writes.** |
| `POST` | `/api/architect/propose` | Run proposal loop — agent may call `propose_patch`. **Nothing written until approved.** |
| `POST` | `/api/architect/apply/{patch_id}` | Human-approved apply: backup → write → `cargo check` |
| `POST` | `/api/architect/reject/{patch_id}` | Discard patch. No disk I/O. |
| `GET`  | `/api/architect/patches` | List pending patches with expiry countdown |

### Dashboard

| Method | Route | Description |
|---|---|---|
| `GET`  | `/` | Dashboard UI (embedded HTML) |
| `GET`  | `/api/health` | CPU %, RAM %, status, active project |
| `POST` | `/api/command` | Text command dispatcher (map project, build project, …) |
| `GET`  | `/api/settings` | Load settings |
| `POST` | `/api/settings` | Save settings |
| `POST` | `/api/settings/api-keys` | Write keys to `.env` |
| `GET`  | `/api/projects` | List saved projects |
| `POST` | `/api/projects/add` | Add a project |
| `POST` | `/api/projects/switch` | Switch active project |
| `GET`  | `/api/generated-projects` | List generated projects |
| `POST` | `/api/projects/enhance` | Improve / add feature to a generated project |
| `GET`  | `/project/{slug}` | Serve live project preview |
| `GET`  | `/api/projects/download/{slug}` | Download project as ZIP |

### Database

| Method | Route | Description |
|---|---|---|
| `POST` | `/api/database/test` | Test SQLite connection |
| `POST` | `/api/database/sqlite/tables` | List tables |
| `POST` | `/api/database/sqlite/schema` | Get table schema |
| `POST` | `/api/database/sqlite/query` | Run SELECT query |
| `POST` | `/api/database/connect` | Test typed connection (SQLite/PostgreSQL/MySQL/…) |
| `POST` | `/api/database/connections/save` | Save connection config |
| `POST` | `/api/database/connections/remove` | Remove connection config |

---

## Project Structure

```
synapse-overlord/
├── src/
│   ├── main.rs                  # Entry point — CLI flag dispatch (web / TUI)
│   ├── web/
│   │   └── mod.rs               # Axum router + embedded dashboard HTML/CSS/JS
│   ├── routes/
│   │   └── hermes.rs            # Hermes Architect handlers
│   │                            #   analyze · propose · apply · reject · list
│   ├── tools/
│   │   ├── mod.rs               # Tool registry, dispatcher, propose_patch handler
│   │   ├── file_tools.rs        # file_list · file_read · file_search (safe I/O)
│   │   ├── patch_tool.rs        # PatchStore — in-memory patch lifecycle
│   │   ├── diff.rs              # Pure-Rust LCS unified diff generator
│   │   └── cargo_check.rs       # cargo check runner
│   ├── llm/
│   │   └── hermes.rs            # System prompt builder · tool-call XML parser
│   ├── builder/                 # Project generation pipeline (12+ templates)
│   ├── enhancer/                # Project improvement pipeline
│   ├── rag/                     # RAG file mapper (role detection, size, imports)
│   ├── agent/                   # Legacy agent pipeline (preserved)
│   ├── sandbox/                 # Rust sandbox executor
│   ├── database/                # SQLite + multi-connector abstraction
│   ├── settings/                # Settings load/save (.synapse-settings.json)
│   ├── safety/                  # Command safety classifier (blocks destructive ops)
│   ├── gitops/                  # Git operations
│   ├── system/                  # CPU/RAM system monitor (sysinfo)
│   ├── models/                  # Triple-model consensus runner
│   └── tui/                     # Ratatui terminal UI
├── generated_projects/          # Output directory for built projects
│   └── <project-slug>/
│       ├── index.html
│       ├── styles.css
│       ├── app.js
│       └── README.md
├── .synapse_backups/            # Auto-backups before every patch apply
├── .env.example                 # Environment variable template
├── Cargo.toml
└── README.md
```

---

## Tech Stack

| Layer | Technology | Version |
|---|---|---|
| Language | Rust | 2024 edition |
| Web framework | Axum | 0.8 |
| Async runtime | Tokio | 1.x (full) |
| LLM inference | Groq API | llama-3.3-70b / hermes models |
| Agent architecture | Custom Hermes tool-use loop | — |
| Project file mapping | Custom RAG scanner | — |
| Database | SQLite via rusqlite | 0.31 (bundled) |
| Diff engine | Pure-Rust LCS unified diff | — |
| Structured logging | tracing + tracing-subscriber | 0.1 / 0.3 |
| Terminal UI | Ratatui + Crossterm | 0.29 / 0.28 |
| Frontend | Vanilla HTML/CSS/JS (embedded) | — |
| HTTP client | reqwest | 0.12 |

---

## Screenshots / Demo

> **To run locally and capture your own screenshots:**
> ```bash
> cargo run -- web   # then open http://127.0.0.1:3000
> ```

### Dashboard — Hermes Architect panel

```
┌──────────────────────────────────────────────┐
│  HERMES ARCHITECT              ✓ READY       │
├──────────────────────────────────────────────┤
│  📁 62 files  🔧 5 tool calls  ⚡ 3.2s  🤖 groq │
├──────────────────────────────────────────────┤
│  TOOL CALLS                                  │
│  ✓ file_list  ✓ file_read  ✓ file_read       │
│  ✓ file_search  ✓ file_read                  │
├──────────────────────────────────────────────┤
│  ◆ SUGGESTIONS                               │
│  › Add integration tests for agent loop      │
│  › Extract dashboard HTML to template file   │
│  › Add rate limiting to analyze endpoint     │
│  › Add OpenTelemetry tracing spans           │
│  › Implement patch TTL cleanup worker        │
├──────────────────────────────────────────────┤
│  ◆ FULL ANALYSIS                             │
│  Synapse-Overlord is a well-structured...    │
│  ...                                         │
└──────────────────────────────────────────────┘
```

---

## Hermes Agent Challenge

<div align="center">

### ◈ Submission: Hermes Agent Challenge

**Project:** Synapse-Overlord  
**Category:** Autonomous Architecture Assistant  
**Primary model:** Hermes / llama-3.3-70b-versatile via Groq  

</div>

### Why this qualifies as a real Hermes Agent application

| Criterion | Implementation |
|---|---|
| **Real tool-use** | Agent calls `file_list`, `file_read`, `file_search` in a true multi-turn loop — not a single-shot prompt |
| **Agentic reasoning** | Model decides which files to open based on previous tool results |
| **Structured output** | `suggestions[]` is a proper array in the JSON response, not raw text |
| **Transparency** | Every tool call name, success flag, and result preview is returned to the caller |
| **Human-in-the-loop** | Patch proposals require explicit HTTP approval — no auto-apply |
| **Production safety** | Path canonicalization, content size limits, TTL-based patch expiry, input validation |
| **Working product** | Full web dashboard, project builder, database explorer — not a toy demo |

### How to verify it's real

```bash
# Start the server
cargo run -- web

# Hit the endpoint
curl -X POST http://127.0.0.1:3000/api/architect/analyze \
  -H "Content-Type: application/json" \
  -d '{"goal":"Map all modules and suggest improvements","max_tool_calls":7}'

# Inspect tool_calls[] in the response — each entry is a real LLM-driven tool invocation
# Inspect suggestions[] — extracted from real model output, not hardcoded
```

### Checklist

- ✅ Hermes agent reads real project files
- ✅ Tool-use loop with up to 7 iterations
- ✅ Structured `suggestions[]` and `tool_calls[]` in JSON response
- ✅ Loading states in dashboard (4 states)
- ✅ "Built with Hermes Agent" badge in UI
- ✅ Safe patch proposal system (propose → approve → apply)
- ✅ `cargo check` passes — zero errors, zero warnings
- ✅ Full web dashboard (not a CLI-only tool)
- ✅ README with architecture diagram and challenge section

---

## Environment Variables

| Variable | Required | Description |
|---|---|---|
| `GROQ_API_KEY` | Yes (for AI features) | Groq API key — get one free at [console.groq.com](https://console.groq.com/) |
| `LLM_BACKEND` | No | Set to `ollama` to use a local model instead of Groq |
| `SYNAPSE_LOGIC_MODEL` | No | Override the logic model (default: `llama-3.3-70b-versatile`) |
| `SYNAPSE_AUDIT_MODEL` | No | Override the audit model |
| `SYNAPSE_OPTIMIZE_MODEL` | No | Override the optimize model |
| `RUST_LOG` | No | Log level filter (e.g. `synapse_overlord=debug`) |

Copy `.env.example` to `.env` and fill in your key.

---

## Safety

Synapse-Overlord is designed to be safe to run on your local machine:

- **No auto-write** — code changes require human approval via a separate HTTP call
- **No deletion** — patch system can only create/overwrite, never delete
- **No shell execution** — only `cargo check` is ever spawned, validated through a safety classifier
- **Path isolation** — all file I/O canonicalized and restricted to project root
- **Size limits** — patch content capped at 256 KB, descriptions at 2 KB
- **Patch TTL** — pending patches expire after 10 minutes, GC'd on next list request
- **Input limits** — goal field capped at 8,192 characters

---

## License

MIT — see [LICENSE](LICENSE)

---

<div align="center">

Built with ❤️ and Rust · Powered by Hermes Agent · Made for the Hermes Agent Challenge

**[⭐ Star this repo](https://github.com/fokrulanthro16-eng/synapse-overlord)** if you find it useful!

</div>
