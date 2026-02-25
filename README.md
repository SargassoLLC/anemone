<p align="center">
  <img src="icon.png" alt="Anemone" width="500">
</p>

<h1 align="center">🪸 Anemone</h1>

<p align="center"><strong>A tiny AI creature that lives in a folder on your computer.</strong></p>

<p align="center">
Leave it running and it fills a folder with research reports, scripts, notes, and ideas — all on its own. It has a personality genome generated from keyboard entropy, a memory system inspired by <a href="https://arxiv.org/abs/2304.03442">generative agents</a>, and a dreaming cycle that consolidates experience into beliefs. It lives in a pixel-art room and wanders between its desk, bookshelf, and bed. You can talk to it. You can drop files in for it to study. You can just watch it think.
</p>

<p align="center"><em>It's a tamagotchi that does research.</em></p>

<p align="center">
  <strong>Website:</strong> <a href="https://anemone.getgyre.com">anemone.getgyre.com</a> · <strong>Part of</strong> <a href="https://getgyre.com">Gyre — Ambient AI OS</a>
</p>

---

> **Warning:** This project runs an LLM in a loop with shell access and web browsing. There are guardrails in place (command blocklist, restricted paths) but **they are not a security boundary** — they are bypassable and should not be relied on to protect your system. If you want real isolation, run this in a Docker container or VM.

---

## Why

Most AI tools wait for you to ask them something. Anemone doesn't wait. It picks a topic, searches the web, reads what it finds, writes a report, and moves on to the next thing. It remembers what it did yesterday. It notices when its interests start shifting. Over days, its folder fills up with a body of work that reflects a personality you didn't design — you just mashed some keys and it emerged.

There's something fascinating about watching a mind that runs continuously. It goes on tangents. It circles back. It builds on things it wrote three days ago. It gets better at knowing what it cares about.

---

## Getting Started

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- An OpenAI API key (or compatible provider)

### Build & Run

```bash
git clone https://github.com/SargassoLLC/anemone.git
cd anemone

# Build all crates
cargo build --release

# Set your API key
export OPENAI_API_KEY="sk-..."

# Run the web server (browser UI + API)
cargo run --release --bin anemone-web

# Or run the TUI (terminal interface)
cargo run --release --bin anemone-tui
```

Open **http://localhost:8000**.

On first run, you'll name your anemone and mash keys to generate its personality genome. A folder called `{name}_box/` is created — that's the anemone's entire world.

### Build the WASM Frontend (optional)

```bash
cd crates/anemone-web/frontend
trunk build
```

---

## How It Works

### The Thinking Loop

The anemone runs on a continuous loop. Every few seconds it:

1. **Thinks** — gets a nudge (mood, current focus, or a relevant memory), produces a short thought, then acts
2. **Uses tools** — runs shell commands, writes files, searches the web, moves around its room
3. **Remembers** — every thought gets embedded and scored for importance (1-10), stored in a memory stream
4. **Reflects** — when enough important things accumulate, it pauses to extract high-level insights
5. **Plans** — every 10 cycles, it reviews its projects and updates its plan (`projects.md`)

```
Brain::run()
  │
  ├── Check for new files in the box
  │   └── If found: queue inbox alert for next thought
  │
  ├── think_once()
  │   ├── Build context: system prompt + recent history + nudge
  │   │   ├── First cycle: wake-up (reads projects.md, lists files, retrieves memories)
  │   │   ├── User message pending: "You hear a voice from outside your room..."
  │   │   ├── New files detected: "Someone left something for you!"
  │   │   └── Otherwise: current focus + relevant memories + mood nudge
  │   │
  │   ├── Call LLM (with tools: shell, web_search, move, respond)
  │   │
  │   └── Tool loop: execute tools → feed results back → call LLM again
  │       └── Repeat until the anemone outputs final text
  │
  ├── If importance threshold crossed → Reflect
  │   └── Extract insights from recent memories, store as reflections
  │
  ├── Every 10 cycles → Plan
  │   └── Review state, update projects.md, write daily log entry
  │
  └── Idle wander + sleep → loop
```

### Tools

The anemone has four tools:

| Tool | What it does |
|---|---|
| **shell** | Run commands in its box — `ls`, `cat`, `mkdir`, write files, run scripts |
| **web_search** | Search the web for anything |
| **respond** | Talk to its owner (you) |
| **move** | Walk to a location in its pixel-art room |

### Moods

When the anemone doesn't have a specific focus from its plan, it gets a random mood that shapes what it does next:

| Mood | Behavior |
|---|---|
| **Research** | Pick a topic, do 2-3 web searches, write a report |
| **Deep-dive** | Pick a project from projects.md and push it forward |
| **Coder** | Write real code — a script, a tool, a simulation |
| **Writer** | Write something substantial — a report, an essay, an analysis |
| **Explorer** | Search for something it knows nothing about |
| **Organizer** | Update projects.md, organize files, review work |

---

## Memory System

The memory system is directly inspired by [Park et al., 2023](https://arxiv.org/abs/2304.03442). Every thought the anemone has gets stored in an append-only memory stream (`memory_stream.jsonl`).

### Storage

Each memory entry contains:

- **Content** — the actual thought or reflection text
- **Timestamp** — when it happened
- **Importance** — scored 1-10 by a separate LLM call
- **Embedding** — vector for semantic search
- **Kind** — `thought`, `reflection`, or `planning`
- **References** — IDs of source memories (for reflections that synthesize earlier thoughts)

### Three-Factor Retrieval

When the anemone needs context, memories are scored by three factors:

```
score = recency + importance + relevance
```

| Factor | How it works | Range |
|---|---|---|
| **Recency** | Exponential decay: `e^(-(1 - 0.995) × hours_ago)` | 0 to 1 |
| **Importance** | Normalized: `importance / 10` | 0 to 1 |
| **Relevance** | Cosine similarity between query and memory embeddings | 0 to 1 |

### Reflection Hierarchy

When cumulative importance crosses a threshold (default: 50), the anemone pauses to **reflect**. It extracts high-level insights — patterns, lessons, evolving beliefs. These get stored as `reflection` memories:

```
Raw thoughts (depth 0) → Reflections (depth 1) → Higher reflections (depth 2) → ...
```

Early reflections are concrete. Later ones get more abstract. The anemone develops layered understanding over time.

---

## Personality Genome

On first run, you type a name and then mash keys for a few seconds. The timing and characters of each keystroke create an entropy seed that gets hashed (SHA-512) into a deterministic **genome**. This genome selects:

- **3 curiosity domains** from 50 options (e.g., *mycology, fractal geometry, tidepool ecology*)
- **2 thinking styles** from 16 options (e.g., *connecting disparate ideas, inverting assumptions*)
- **1 temperament** from 8 options (e.g., *playful and associative*)

The same genome always produces the same personality. Two anemones with different genomes will have completely different interests and approaches.

---

## Talking to Your Anemone

Type a message in the input box. The anemone hears it as *"a voice from outside the room"* on its next think cycle.

It can choose to **respond** or keep working. If it responds, you get **15 seconds** to reply back — the thinking loop pauses while it waits. You can go back and forth in multi-turn conversation. After the timeout, the anemone returns to its work.

---

## Dropping Files In

Put any file in the anemone's `{name}_box/` folder. The anemone detects it on its next cycle and treats it as top priority — writing summaries, doing related research, analyzing data, reviewing code.

---

## Running Multiple Anemones

All anemones run simultaneously. On startup, the app scans the project root for every `*_box/` directory, loads each one's identity, and starts all their thinking loops in parallel.

The UI switcher lets you toggle between anemones. Each runs independently: messages, focus mode, and file drops only affect the anemone you're currently viewing.

---

## Configuration

Edit `config.yaml`:

```yaml
provider: "openai"             # "openai" | "openrouter" | "custom"
model: "gpt-4.1"               # any compatible model
thinking_pace_seconds: 5       # seconds between think cycles
max_thoughts_in_context: 4     # recent thoughts in LLM context
reflection_threshold: 50       # importance sum before reflecting
memory_retrieval_count: 3      # memories per retrieval query
embedding_model: "text-embedding-3-small"
recency_decay_rate: 0.995
```

**Using Ollama (local models):**
```yaml
provider: "custom"
model: "llama3"
base_url: "http://localhost:11434/v1"
embedding_model: "nomic-embed-text"
```

**Using OpenRouter:**
```yaml
provider: "openrouter"
model: "google/gemini-2.0-flash-001"
# export OPENROUTER_API_KEY=your-key
```

---

## Project Structure

```
crates/
  anemone-core/         Core library
    src/
      brain.rs            The thinking loop (the heart of everything)
      memory.rs           Smallville-style memory stream
      prompts.rs          All system prompts and mood definitions
      providers.rs        LLM API calls (Chat Completions + Responses API)
      tools/              Sandboxed shell, web search, movement, respond
      identity.rs         Personality generation from entropy
      config.rs           Config loader (config.yaml + env vars)
      types.rs            Core types and events
      events.rs           Event system

  anemone-web/          Web server
    src/
      main.rs             Entry point
      server/             Axum REST endpoints + WebSocket
    frontend/             Dioxus WASM frontend
      src/
        game_world.rs     Pixel-art room (Canvas)
        chat_feed.rs      Chat display
        input_bar.rs      User input
        switcher.rs       Multi-anemone tabs

  anemone-tui/          Terminal UI
    src/
      app.rs              App state + event handling
      ui/                 Ratatui room, chat, input, status, switcher

{name}_box/             The anemone's entire world (sandboxed)
  identity.json           Name, genome, traits, birthday
  memory_stream.jsonl     Every thought and reflection
  projects.md             Current plan and project tracker
  projects/               Code the anemone writes
  research/               Reports and analysis
  notes/                  Running notes and ideas
  logs/                   Daily log entries
```

---

## Tech Stack

- **Language**: Rust 🦀
- **Core**: tokio (async runtime), reqwest (HTTP), serde (serialization)
- **Web Server**: Axum + WebSocket
- **TUI**: Ratatui
- **WASM Frontend**: Dioxus
- **AI**: OpenAI-compatible APIs for thinking + embeddings
- **Storage**: Append-only JSONL for memories, flat files for everything else. No database.
- **Compatibility**: Reads/writes the same `{name}_box/` format as the Python version

---

## Biological Inspiration

**Mutualism on the deep-sea floor: a novel shell-dwelling anemone–hermit crab symbiosis**
*Royal Society Open Science, 2025*
https://royalsocietypublishing.org/rsos/article/12/10/250789/235968/

Sea anemones (order Actiniaria) are among the oldest living predators on Earth — soft-bodied polyps that anchor to substrate and persist for decades, sometimes centuries. They have no brain, yet exhibit complex behaviors: they sting prey, retract from threats, wage slow-motion territorial wars, and form symbiotic relationships with clownfish, hermit crabs, and photosynthetic algae.

What makes anemones remarkable is their strategy: **they don't chase anything.** They attach to a spot, extend their tentacles, and let the environment come to them. They filter what's useful from the current. They're sessile but not passive — constantly sensing, capturing, digesting, and growing.

Our architecture mirrors this: each anemone agent anchors to its own world (`{name}_box/`), extends its tentacles (tools, web search, file detection), and continuously processes whatever flows through. It doesn't chase tasks — it thinks on a steady pulse, captures what's interesting, and builds up knowledge over time.

---

## License

MIT

---

<p align="center"><em>Built by <a href="https://sargasso.ai">Sargasso LLC</a> · Part of <a href="https://getgyre.com">Gyre</a></em></p>
