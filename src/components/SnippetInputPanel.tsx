import { useEffect, useRef, useState } from 'react';
import { useAppStore } from '../stores/app';

/**
 * Asks for the `{input:Prompt}` values of a snippet before it is
 * pasted. Enter moves to the next field and pastes from the last
 * one; Escape cancels without pasting.
 */
export function SnippetInputPanel() {
  const { pendingSnippet, submitSnippetInputs, cancelSnippetInputs } = useAppStore();
  const prompts = pendingSnippet?.prompts ?? [];
  const [values, setValues] = useState<Record<string, string>>({});
  const fieldRefs = useRef<(HTMLInputElement | null)[]>([]);

  useEffect(() => {
    fieldRefs.current[0]?.focus();
  }, [pendingSnippet]);

  if (!pendingSnippet) return null;

  const submit = () => submitSnippetInputs(values);

  const handleKeyDown = (e: React.KeyboardEvent, index: number) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelSnippetInputs();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (index < prompts.length - 1) {
        fieldRefs.current[index + 1]?.focus();
      } else {
        submit();
      }
    }
  };

  return (
    <form
      className="p-4 border-t border-[var(--border)]"
      aria-label="Snippet inputs"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <div className="flex justify-between items-center mb-3">
        <h3 className="font-semibold">Fill in snippet</h3>
        <span className="text-xs text-gray-500">Enter to paste · Esc to cancel</span>
      </div>
      <div className="space-y-2">
        {prompts.map((prompt, i) => (
          <div key={prompt}>
            <label htmlFor={`snippet-input-${i}`} className="text-xs text-gray-500 mb-1 block">
              {prompt}
            </label>
            <input
              id={`snippet-input-${i}`}
              ref={(el) => {
                fieldRefs.current[i] = el;
              }}
              type="text"
              value={values[prompt] ?? ''}
              onChange={(e) => setValues((v) => ({ ...v, [prompt]: e.target.value }))}
              onKeyDown={(e) => handleKeyDown(e, i)}
              className="w-full px-3 py-1.5 text-sm bg-[var(--input-bg)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>
        ))}
      </div>
    </form>
  );
}
