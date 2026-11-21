import type { ProjectSummary } from './api';

export interface Command {
  id: string;
  label: string;
  shortcut?: string;
  group?: string;
  run: () => void | Promise<void>;
  enabled: () => boolean;
}

/// Premiere-style command registry: every action a panel exposes is a command
/// with an id, a label, a shortcut and an enabled predicate. Automation (and
/// the palette) can do everything a click does, like the engine pattern the
/// suite documents.
class Registry {
  #items = new Map<string, Command>();

  register(command: Command) {
    this.#items.set(command.id, command);
    return () => this.#items.delete(command.id);
  }

  list(): Command[] {
    return [...this.#items.values()].sort((a, b) => a.label.localeCompare(b.label, 'it'));
  }

  get(id: string) {
    return this.#items.get(id);
  }

  async run(id: string) {
    const command = this.#items.get(id);
    if (!command || !command.enabled()) return false;
    await command.run();
    return true;
  }

  shortcutKeys(): Map<string, string> {
    const map = new Map<string, string>();
    for (const command of this.#items.values()) {
      if (command.shortcut) map.set(command.shortcut.toUpperCase(), command.id);
    }
    return map;
  }
}

export const commands = new Registry();

export function formatShortcut(event: KeyboardEvent): string | null {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push('Ctrl');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  const key = event.key === ' ' ? 'Space' : event.key.length === 1 ? event.key.toUpperCase() : event.key;
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(key)) return null;
  parts.push(key);
  return parts.join('+');
}

export interface Workspace {
  id: string;
  label: string;
  shortcut: string;
}

export const WORKSPACES: Workspace[] = [
  { id: 'materials', label: 'Progetti e materiali', shortcut: 'Alt+1' },
  { id: 'edit', label: 'Montaggio', shortcut: 'Alt+2' },
  { id: 'social', label: 'Shorts e Social', shortcut: 'Alt+3' },
  { id: 'story', label: 'Storie e Regia AI', shortcut: 'Alt+4' },
];

export interface LayoutState {
  monitorSplit: number;
  timelineHeight: number;
  inspectorWidth: number;
  workspace: string;
}

const LAYOUT_KEY = 'nl-shell.layout.v1';

export function loadLayout(): LayoutState {
  const fallback: LayoutState = {
    monitorSplit: 0.5,
    timelineHeight: 0.4,
    inspectorWidth: 0.24,
    workspace: 'edit',
  };
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    return raw ? { ...fallback, ...JSON.parse(raw) } : fallback;
  } catch {
    return fallback;
  }
}

export function saveLayout(layout: LayoutState) {
  try {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
  } catch {
    /* the shell must survive a blocked storage */
  }
}

export function currentProjectId(): string | null {
  return localStorage.getItem('nl-project');
}

export function rememberProject(id: string | null) {
  if (id) localStorage.setItem('nl-project', id);
  else localStorage.removeItem('nl-project');
}

export function pickProjectSummary(projects: ProjectSummary[], id: string | null): ProjectSummary | null {
  if (!id) return null;
  return projects.find((p) => p.id === id) ?? null;
}
