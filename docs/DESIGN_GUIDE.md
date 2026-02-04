# Second Brain - Design Guide

## Overview

Second Brain is a real-time meeting assistant with a **premium, "bento-box" style UI** built with Tauri v2, Svelte 5, and Tailwind CSS v4.

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| Desktop Framework | Tauri v2 |
| Frontend | Svelte 5 (Runes) |
| Styling | **Tailwind CSS v4** (CSS-first config) |
| Backend | Rust |
| Database | SurrealDB (embedded) |
| AI/ML | sherpa-rs (ASR), GLiNER (NER), EmbeddingGemma, rig.rs (LLM) |

---

## Design System

### 🎨 Colors & Theme

We use a deep, OLED-friendly dark mode based on the **Zinc** palette, with **Indigo** accents.

| Role | Tailwind Class | Hex |
|------|----------------|-----|
| **Background** | `bg-zinc-950` | `#09090b` |
| **Panel BG** | `bg-zinc-900/50` | `#18181b` (50% opacity) |
| **Border** | `border-white/5` | `rgba(255,255,255,0.05)` |
| **Text Primary** | `text-zinc-100` | `#f4f4f5` |
| **Text Muted** | `text-zinc-400` | `#a1a1aa` |
| **Accent** | `text-indigo-400` | `#818cf8` |
| **Accent BG** | `bg-indigo-500/10` | `rgba(99,102,241,0.1)` |

### 🔤 Typography

- **Primary Font**: `Inter` (Google Fonts) - Clean, modern sans-serif.
- **Monospace**: `JetBrains Mono` - For code, IDs, and technical data.

```css
/* app.css */
@theme {
  --font-sans: "Inter", sans-serif;
  --font-mono: "JetBrains Mono", monospace;
}
```

### 🧱 Layout: Floating Sidebar (Bento Box)

The app uses a "floating" layout where the sidebar and main content are detached panels with rounded corners, sitting on a deep background.

```html
<div class="flex h-screen bg-zinc-950 p-3 gap-3">
  <!-- Sidebar Panel -->
  <nav class="w-64 bg-zinc-900/50 backdrop-blur-2xl rounded-2xl border border-white/5 shadow-2xl">
    ...
  </nav>

  <!-- Main Content Panel -->
  <main class="flex-1 bg-zinc-900/20 rounded-2xl border border-white/5 shadow-2xl relative overflow-hidden">
    ...
  </main>
</div>
```

### ✨ Effects

- **Glassmorphism**: `backdrop-blur-xl` or `backdrop-blur-2xl` on panels.
- **Shadows**: `shadow-2xl` + `shadow-black/20` for depth.
- **Borders**: Subtle `border-white/5` to define edges without harsh lines.
- **Pulsing**: `animate-pulse` for live recording states.

---

## Component Patterns

### Cards

Cards use a subtle background with a hover effect.

```svelte
<div class="bg-zinc-900/30 border border-white/5 rounded-2xl p-5 hover:bg-zinc-900/50 transition-colors">
  <div class="flex justify-between items-center mb-4">
    <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Title</h2>
  </div>
  <!-- Content -->
</div>
```

### Buttons

**Primary (Accent)**
```svelte
<button class="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white text-sm font-medium rounded-lg transition-colors">
  Action
</button>
```

**Secondary (Glass)**
```svelte
<button class="px-3 py-2 bg-white/5 hover:bg-white/10 border border-white/5 rounded-lg text-sm text-zinc-300 transition-colors">
  Cancel
</button>
```

### Badges

**Status Badge**
```svelte
<span class="text-[10px] font-bold px-2 py-1 rounded uppercase tracking-wide bg-amber-500/10 text-amber-400">
  Pending
</span>
```

**Live Indicator**
```svelte
<span class="text-[10px] font-bold px-1.5 py-0.5 rounded-full bg-red-500/20 text-red-400 animate-pulse">
  LIVE
</span>
```

---

## App Structure

### Navigation

The sidebar controls the `activeView` state.

| View | Icon | Description |
|------|------|-------------|
| **Home** | 🏠 | Dashboard with agenda & insights |
| **Meetings** | 📅 | List of past recordings |
| **Notes** | 📝 | Quick notes & tags |
| **Insights** | 💡 | AI analysis & patterns |
| **Integrations** | 🔌 | Tool connections |

### Adding a New View

1.  **Update State**: Add the view key to `activeView` in `SecondBrain.svelte`.
2.  **Add Sidebar Item**:
    ```svelte
    <button 
      class="flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all {activeView === 'new' ? 'bg-indigo-500/10 text-indigo-400' : 'text-zinc-400 hover:bg-white/5'}"
      onclick={() => activeView = 'new'}
    >
      <span>🆕</span>
      <span>New View</span>
    </button>
    ```
3.  **Add Content Section**:
    ```svelte
    {:else if activeView === 'new'}
      <div class="max-w-5xl mx-auto p-8">
        <!-- Content -->
      </div>
    {/if}
    ```

---

## State Management

We use **Svelte 5 Runes** for local state and Tauri commands for backend data.

```typescript
// Local UI State
let activeView = $state('home');
let searchQuery = $state("");

// Backend Data
let notes = $state<Note[]>([]);

// Derived State
let filteredNotes = $derived(
  notes.filter(n => n.content.includes(searchQuery))
);

// Effects
$effect(() => {
  console.log("View changed to:", activeView);
});
```

---

## Development

### Run Dev Server
```bash
bun tauri dev
```

### Build for Production
```bash
bun tauri build
```
