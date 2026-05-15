# Synapse-Overlord

Synapse-Overlord is an offline/local AI project builder dashboard built with Rust.

It can generate, preview, download, and improve static web projects from natural-language commands.

This is Version 1: a fully local/offline builder. Real AI API mode will be added in Version 2.

## Features

- Localhost web dashboard
- Offline project generation
- Generated projects gallery
- In-app project preview
- Download generated projects as ZIP
- Copy project path
- README viewer
- Project enhancer
- Backup before project edits
- Rust sandbox test
- Project structure mapper
- Offline model fallback
- No API key required for Version 1

## Supported Project Types

Synapse can currently generate:

- Medical shop landing page
- Portfolio website
- Ecommerce product page
- Generic landing page
- Restaurant/menu website
- Student dashboard
- Quiz app
- Notes/todo app
- AI chatbot frontend
- Admin dashboard
- File tracking dashboard
- Expense tracker

Unknown project ideas fall back to an adaptive offline generator.

## Example Commands

Use these inside the Synapse dashboard command box:

- build project medical shop landing page with medicine search
- build project portfolio website for a full stack developer
- build project ecommerce product page with cart
- build project restaurant ordering website with menu categories and cart
- build project student dashboard with attendance marks notices and course cards
- build project quiz app with score timer and restart
- build project notes app with localStorage and completed filter
- build project AI chatbot frontend with message history and mock replies
- build project admin dashboard with analytics cards table and activity feed
- build project file tracking dashboard with search filters status badges and timeline
- build project expense tracker with income expense balance chart and localStorage

Project improvement commands:

- improve project medical-shop add dark mode toggle and premium medicine cards
- add feature medical-shop shopping cart with localStorage

Other commands:

- map project
- test sandbox
- ask models
- run agent

## Tech Stack

- Rust
- Tokio
- Axum
- Ratatui
- Crossterm
- Sysinfo
- SQLite foundation
- Local HTML/CSS/JS project generation

## Getting Started

Clone the repository:

git clone https://github.com/fokrulanthro16-eng/synapse-overlord.git
cd synapse-overlord

Run the web dashboard:

cargo run -- web

Open:

http://localhost:3000

Run the terminal TUI:

cargo run

## Generated Projects

Generated projects are saved inside:

generated_projects/

Each generated project includes:

- index.html
- styles.css
- app.js
- README.md

Dashboard actions:

- Preview
- Download ZIP
- Copy Path
- README
- Improve
- Add Feature

## Safety

Synapse V1 is designed to run locally and safely.

- No real AI API key required
- No external API required for project generation
- No destructive system commands
- No auto Git commit
- Generated projects are not overwritten
- Project edits create backups before modifying files

## Environment Variables

The .env file is ignored by Git and should not be committed.

Version 2 may use:

GROQ_API_KEY=
SYNAPSE_LOGIC_MODEL=
SYNAPSE_AUDIT_MODEL=
SYNAPSE_OPTIMIZE_MODEL=

## Roadmap

Version 1:

- Offline project builder
- Generated projects dashboard
- Preview and download
- Project enhancer
- Local command system

Version 2:

- Real AI API mode
- Groq/OpenAI-compatible model calls
- Triple-model consensus
- API-key powered custom generation
- Real database integrations
- IDE integrations

## Status

Synapse-Overlord V1 is a working offline MVP.

## License

No license selected yet.
