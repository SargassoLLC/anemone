# 🪸 Anemone Patterns

Structured prompt templates for anemone agents, tidal pool management, and the Gyre immune system. Inspired by and adapted from [Fabric](https://github.com/danielmiessler/fabric) by Daniel Miessler.

## Structure

```
patterns/
├── drain/                    ← Tidal pool drain engine
│   ├── distill_surface.md    ← Distill conversations into pool surface updates
│   ├── extract_core_fact.md  ← Single-fact extraction for anemone micro-drains
│   └── summarize_midwater.md ← Structured midwater entry generation
│
├── research/                 ← Anemone research moods
│   ├── extract_wisdom.md     ← Pull insights, ideas, quotes, facts from content
│   ├── analyze_paper.md      ← Structured paper analysis with rigor scoring
│   ├── extract_patterns.md   ← Find recurring patterns across sources
│   └── capture_thinker.md    ← Profile a thinker's philosophy and contributions
│
└── immune/                   ← Gyre immune system
    ├── verify_claims.md      ← Fact-check agent responses against pool surface
    └── detect_drift.md       ← Detect systematic inaccuracy patterns over time
```

## Design Principles

Borrowed from Fabric:
- **16-word bullet maximum** — forces compression, fights context bloat
- **Identity + Steps + Output Instructions** — clean, repeatable prompt structure
- **JSON output for machine consumption** — drain and immune patterns return structured data
- **Markdown output for human consumption** — research patterns return readable reports

Added for Gyre:
- **Pool integration instructions** — each pattern knows how to route output to tidal pools
- **Importance scoring** — research patterns score discoveries for drain routing (7+ → midwater, 9+ → surface)
- **Immune system hooks** — verify_claims returns action triggers (pass/correct/flag/rollback)
- **Antibody generation** — detect_drift outputs suggested post-conditions

## Usage

Patterns are used by:
1. **Anemone brain loop** — research patterns guide think cycles
2. **Tidal pool drain engine** — drain patterns process conversations into pool updates
3. **Gyre immune system** — immune patterns verify and monitor agent quality
4. **Strands AI Functions** — patterns become `@ai_function` post-conditions

## Attribution

Core patterns adapted from [Fabric](https://github.com/danielmiessler/fabric) (MIT License) by Daniel Miessler. Modified for tidal pool architecture and Gyre immune system integration.
