import type { SearchResult } from '../types';

interface ResultItemProps {
  result: SearchResult;
  isSelected: boolean;
  onClick: () => void;
  index: number;
}

function AppIcon() {
  return (
    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-green-400 to-green-600 flex items-center justify-center">
      <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <rect x="3" y="3" width="18" height="18" rx="2" />
        <path d="M3 9h18" />
        <path d="M9 21V9" />
      </svg>
    </div>
  );
}

function WebIcon() {
  return (
    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-blue-400 to-blue-600 flex items-center justify-center">
      <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <circle cx="12" cy="12" r="10" />
        <path d="M2 12h20" />
        <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
      </svg>
    </div>
  );
}

function CommandIcon() {
  return (
    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-amber-400 to-orange-500 flex items-center justify-center">
      <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M12 20a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z" />
        <path d="M12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z" />
        <path d="M12 2v2M12 20v2M2 12h2M20 12h2" />
      </svg>
    </div>
  );
}

function ClipboardIcon() {
  return (
    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-teal-400 to-cyan-500 flex items-center justify-center">
      <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
        <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
      </svg>
    </div>
  );
}

function NoteIcon() {
  return (
    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-purple-400 to-purple-600 flex items-center justify-center">
      <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
        <polyline points="14 2 14 8 20 8" />
        <line x1="16" y1="13" x2="8" y2="13" />
        <line x1="16" y1="17" x2="8" y2="17" />
        <line x1="10" y1="9" x2="8" y2="9" />
      </svg>
    </div>
  );
}

function FileIcon() {
  return (
    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-indigo-400 to-indigo-600 flex items-center justify-center">
      <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
      </svg>
    </div>
  );
}

function DefaultIcon() {
  return (
    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-gray-400 to-gray-600 flex items-center justify-center">
      <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <path d="M14 2v6h6" />
      </svg>
    </div>
  );
}

export function ResultItem({ result, isSelected, onClick, index }: ResultItemProps) {
  const getIcon = () => {
    switch (result.result_type) {
      case 'application':
        return <AppIcon />;
      case 'web_search':
        return <WebIcon />;
      case 'system_command':
        return <CommandIcon />;
      case 'clipboard':
        return <ClipboardIcon />;
      case 'note':
        return <NoteIcon />;
      case 'file':
        return <FileIcon />;
      default:
        return <DefaultIcon />;
    }
  };

  const getTypeLabel = () => {
    switch (result.result_type) {
      case 'application':
        return 'App';
      case 'web_search':
        return 'Web';
      case 'system_command':
        return 'Cmd';
      case 'clipboard':
        return 'Clip';
      case 'note':
        return 'Note';
      case 'file':
        return 'File';
      default:
        return '';
    }
  };

  return (
    <div
      onClick={onClick}
      style={{ animationDelay: `${index * 30}ms` }}
      className={`flex items-center px-4 py-3 cursor-pointer transition-all duration-150 animate-fade-slide-in opacity-0 ${
        isSelected
          ? 'bg-blue-500/10 border-l-2 border-blue-500'
          : 'hover:bg-[var(--selected)] border-l-2 border-transparent'
      }`}
    >
      {getIcon()}
      <div className="flex-1 min-w-0 ml-3">
        <div className="font-medium truncate">{result.name}</div>
        <div className="text-xs text-gray-500 truncate">{result.description}</div>
      </div>
      <div className="text-[10px] uppercase tracking-wider text-gray-400 font-medium px-2 py-1 rounded bg-[var(--input-bg)]">
        {getTypeLabel()}
      </div>
    </div>
  );
}
