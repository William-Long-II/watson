import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Scene, SceneInput, SceneStep } from '../types';
import { ConfirmModal } from './ConfirmModal';

/**
 * Phase 2a (#74): Settings section for Scenes.
 *
 * A Scene is a named, ordered list of steps (launch app, open URL,
 * focus window, run command) that runs with one Enter from the
 * launcher. Same shape as `SnippetsSettings`: a list of rows, one
 * inline editor for create/edit, and a confirm before delete. Each
 * row also has a Run button so a Scene can be tried without leaving
 * Settings.
 */

export const DEFAULT_INTER_STEP_DELAY_MS = 200;

const STEP_TYPES: { value: SceneStep['type']; label: string }[] = [
  { value: 'launch_app', label: 'Launch app' },
  { value: 'open_url', label: 'Open URL' },
  { value: 'focus_window', label: 'Focus window' },
  { value: 'run_command', label: 'Run command' },
];

export function emptyStep(type: SceneStep['type']): SceneStep {
  switch (type) {
    case 'launch_app':
      return { type, path: '' };
    case 'open_url':
      return { type, url: '' };
    case 'focus_window':
      return { type, app: '', title: '' };
    case 'run_command':
      return { type, command: '' };
  }
}

/** The field that must be non-empty for a step to be runnable. */
function stepTarget(step: SceneStep): string {
  switch (step.type) {
    case 'launch_app':
      return step.path;
    case 'open_url':
      return step.url;
    case 'focus_window':
      return step.app;
    case 'run_command':
      return step.command;
  }
}

export function isSceneInputValid(input: SceneInput): boolean {
  return (
    input.name.trim().length > 0 &&
    input.steps.length > 0 &&
    input.steps.every((s) => stepTarget(s).trim().length > 0)
  );
}

export function ScenesSettings() {
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [editing, setEditing] = useState<Scene | null>(null);
  const [isAdding, setIsAdding] = useState(false);
  const [deletingScene, setDeletingScene] = useState<Scene | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const list = await invoke<Scene[]>('list_scenes');
      setScenes(list);
    } catch (e) {
      console.error('Failed to list scenes:', e);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const handleSave = async (scene: SceneInput) => {
    try {
      if (editing) {
        await invoke<Scene>('update_scene', { id: editing.id, scene });
      } else {
        await invoke<Scene>('create_scene', { scene });
      }
      setError(null);
      setEditing(null);
      setIsAdding(false);
      await refresh();
    } catch (e) {
      // Backend validation errors ("step 2: step is missing its URL")
      // are user-readable; show them under the editor.
      setError(String(e));
    }
  };

  const runScene = async (scene: Scene) => {
    try {
      await invoke('execute_action', { action: { type: 'run_scene', scene_id: scene.id } });
    } catch (e) {
      setError(String(e));
    }
  };

  const confirmDelete = async () => {
    if (!deletingScene) return;
    const id = deletingScene.id;
    setDeletingScene(null);
    try {
      await invoke('delete_scene', { id });
      setEditing(null);
      await refresh();
    } catch (e) {
      console.error('Failed to delete scene:', e);
    }
  };

  const closeEditor = () => {
    setEditing(null);
    setIsAdding(false);
    setError(null);
  };

  return (
    <div>
      <div className="flex justify-between items-center mb-2">
        <label className="text-sm text-gray-500">Scenes</label>
        {!isAdding && editing === null && (
          <button
            type="button"
            onClick={() => setIsAdding(true)}
            className="text-xs text-blue-500 hover:text-blue-600"
          >
            + Add Scene
          </button>
        )}
      </div>
      <p className="text-xs text-gray-400 mb-2">
        A Scene opens a set of apps, URLs and windows together. Search its name in Watson and press Enter
        to run every step in order.
      </p>

      <div className="space-y-2">
        {isAdding && (
          <SceneEditor scene={null} error={error} onSave={handleSave} onCancel={closeEditor} />
        )}

        {scenes.map((s) =>
          editing?.id === s.id ? (
            <SceneEditor
              key={s.id}
              scene={s}
              error={error}
              onSave={handleSave}
              onCancel={closeEditor}
              onDelete={() => setDeletingScene(s)}
            />
          ) : (
            <div
              key={s.id}
              className="flex items-center justify-between p-2 bg-[var(--input-bg)] rounded-lg hover:bg-[var(--selected)] transition-colors"
            >
              <button
                type="button"
                onClick={() => !isAdding && setEditing(s)}
                className="flex items-center gap-2 min-w-0 flex-1 text-left"
                aria-label={`Edit scene ${s.name}`}
              >
                {s.icon && <span className="shrink-0">{s.icon}</span>}
                <span className="text-sm truncate">{s.name}</span>
                <span className="text-xs text-gray-400 shrink-0">
                  {s.steps.length} {s.steps.length === 1 ? 'step' : 'steps'}
                </span>
              </button>
              <button
                type="button"
                onClick={() => runScene(s)}
                aria-label={`Run scene ${s.name}`}
                className="ml-2 px-2 py-1 text-xs text-blue-500 hover:text-blue-600 rounded"
              >
                Run
              </button>
            </div>
          ),
        )}

        {scenes.length === 0 && !isAdding && (
          <p className="text-xs text-gray-400 italic">No scenes yet &mdash; add one like &ldquo;Start Work&rdquo;.</p>
        )}
      </div>

      <ConfirmModal
        open={deletingScene !== null}
        title="Delete this scene?"
        message={deletingScene ? `"${deletingScene.name}" will be removed. This can't be undone.` : ''}
        confirmLabel="Delete"
        variant="danger"
        onConfirm={confirmDelete}
        onCancel={() => setDeletingScene(null)}
      />
    </div>
  );
}

interface SceneEditorProps {
  scene: Scene | null;
  error: string | null;
  onSave: (scene: SceneInput) => void;
  onCancel: () => void;
  onDelete?: () => void;
}

const inputClass =
  'w-full px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500';

function SceneEditor({ scene, error, onSave, onCancel, onDelete }: SceneEditorProps) {
  const [name, setName] = useState(scene?.name ?? '');
  const [icon, setIcon] = useState(scene?.icon ?? '');
  const [delay, setDelay] = useState(scene?.inter_step_delay_ms ?? DEFAULT_INTER_STEP_DELAY_MS);
  const [steps, setSteps] = useState<SceneStep[]>(scene?.steps ?? [emptyStep('launch_app')]);

  const input: SceneInput = {
    name,
    icon: icon.trim() ? icon.trim() : null,
    steps,
    inter_step_delay_ms: delay,
  };
  const isValid = isSceneInputValid(input);

  const updateStep = (index: number, step: SceneStep) =>
    setSteps((prev) => prev.map((s, i) => (i === index ? step : s)));
  const removeStep = (index: number) => setSteps((prev) => prev.filter((_, i) => i !== index));
  const moveStep = (index: number, delta: number) =>
    setSteps((prev) => {
      const target = index + delta;
      if (target < 0 || target >= prev.length) return prev;
      const next = [...prev];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });

  return (
    <div className="space-y-2 p-3 bg-[var(--input-bg)] rounded-lg">
      <div className="flex gap-2">
        <div className="w-16">
          <label htmlFor="scene-icon" className="text-xs text-gray-500 mb-1 block">
            Icon
          </label>
          <input
            id="scene-icon"
            type="text"
            value={icon}
            onChange={(e) => setIcon(e.target.value)}
            placeholder="💼"
            className={inputClass}
          />
        </div>
        <div className="flex-1">
          <label htmlFor="scene-name" className="text-xs text-gray-500 mb-1 block">
            Name
          </label>
          <input
            id="scene-name"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Start Work"
            className={inputClass}
          />
        </div>
      </div>

      <div>
        <label className="text-xs text-gray-500 mb-1 block">Steps (run top to bottom)</label>
        <ol className="space-y-2">
          {steps.map((step, i) => (
            <li key={i} className="flex gap-2 items-start">
              <span className="text-xs text-gray-400 w-4 pt-2 shrink-0">{i + 1}.</span>
              <div className="flex-1 space-y-1">
                <select
                  aria-label={`Step ${i + 1} type`}
                  value={step.type}
                  onChange={(e) => updateStep(i, emptyStep(e.target.value as SceneStep['type']))}
                  className={inputClass}
                >
                  {STEP_TYPES.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </select>
                <StepFields step={step} index={i} onChange={(s) => updateStep(i, s)} />
              </div>
              <div className="flex flex-col gap-1 shrink-0">
                <button
                  type="button"
                  aria-label={`Move step ${i + 1} up`}
                  disabled={i === 0}
                  onClick={() => moveStep(i, -1)}
                  className="px-1 text-xs text-gray-400 hover:text-gray-600 disabled:opacity-30"
                >
                  ▲
                </button>
                <button
                  type="button"
                  aria-label={`Move step ${i + 1} down`}
                  disabled={i === steps.length - 1}
                  onClick={() => moveStep(i, 1)}
                  className="px-1 text-xs text-gray-400 hover:text-gray-600 disabled:opacity-30"
                >
                  ▼
                </button>
                <button
                  type="button"
                  aria-label={`Remove step ${i + 1}`}
                  onClick={() => removeStep(i)}
                  className="px-1 text-xs text-red-400 hover:text-red-600"
                >
                  ✕
                </button>
              </div>
            </li>
          ))}
        </ol>
        <button
          type="button"
          onClick={() => setSteps((prev) => [...prev, emptyStep('launch_app')])}
          className="mt-2 text-xs text-blue-500 hover:text-blue-600"
        >
          + Add step
        </button>
      </div>

      <div className="flex items-center gap-2">
        <label htmlFor="scene-delay" className="text-xs text-gray-500">
          Delay between steps (ms)
        </label>
        <input
          id="scene-delay"
          type="number"
          min={0}
          max={10000}
          step={50}
          value={delay}
          onChange={(e) => setDelay(Math.max(0, Number(e.target.value) || 0))}
          className="w-24 px-2 py-1 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>

      {error && (
        <p role="alert" className="text-xs text-red-500">
          {error}
        </p>
      )}

      <div className="flex gap-2 pt-1">
        <button
          type="button"
          onClick={() => isValid && onSave(input)}
          disabled={!isValid}
          className="px-3 py-1.5 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          Save
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-1.5 text-sm bg-[var(--selected)] rounded-lg hover:bg-[var(--border)] transition-colors"
        >
          Cancel
        </button>
        {onDelete && (
          <button
            type="button"
            onClick={onDelete}
            className="px-3 py-1.5 text-sm text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors ml-auto"
          >
            Delete
          </button>
        )}
      </div>
    </div>
  );
}

function StepFields({
  step,
  index,
  onChange,
}: {
  step: SceneStep;
  index: number;
  onChange: (step: SceneStep) => void;
}) {
  const label = `Step ${index + 1}`;
  switch (step.type) {
    case 'launch_app':
      return (
        <input
          aria-label={`${label} app path`}
          value={step.path}
          onChange={(e) => onChange({ ...step, path: e.target.value })}
          placeholder="/Applications/Slack.app or C:\Program Files\…\app.exe"
          className={`${inputClass} font-mono`}
        />
      );
    case 'open_url':
      return (
        <input
          aria-label={`${label} URL`}
          value={step.url}
          onChange={(e) => onChange({ ...step, url: e.target.value })}
          placeholder="https://calendar.google.com"
          className={`${inputClass} font-mono`}
        />
      );
    case 'focus_window':
      return (
        <div className="flex gap-1">
          <input
            aria-label={`${label} window app`}
            value={step.app}
            onChange={(e) => onChange({ ...step, app: e.target.value })}
            placeholder="App (e.g. Code)"
            className={inputClass}
          />
          <input
            aria-label={`${label} window title contains`}
            value={step.title}
            onChange={(e) => onChange({ ...step, title: e.target.value })}
            placeholder="Title contains (optional)"
            className={inputClass}
          />
        </div>
      );
    case 'run_command':
      return (
        <input
          aria-label={`${label} command`}
          value={step.command}
          onChange={(e) => onChange({ ...step, command: e.target.value })}
          placeholder="Command id (e.g. maximize, lock)"
          className={`${inputClass} font-mono`}
        />
      );
  }
}
