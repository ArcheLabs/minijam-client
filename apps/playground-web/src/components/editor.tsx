import Editor, { loader, type OnMount } from "@monaco-editor/react";
import * as monaco from "monaco-editor";

loader.config({ monaco });

interface CodeEditorProps {
  language: "c" | "cpp";
  value: string;
  onChange(value: string): void;
}

export function CodeEditor({ language, value, onChange }: CodeEditorProps) {
  const onMount: OnMount = (editor) => {
    editor.focus();
  };
  return (
    <div className="editor-shell" aria-label="Service source editor">
      <Editor
        height="100%"
        language={language === "cpp" ? "cpp" : "c"}
        value={value}
        onChange={(next) => onChange(next ?? "")}
        onMount={onMount}
        theme="vs-dark"
        options={{
          minimap: { enabled: false },
          fontFamily: "'IBM Plex Mono', 'SFMono-Regular', Consolas, monospace",
          fontSize: 14,
          lineHeight: 22,
          padding: { top: 16 },
          scrollBeyondLastLine: false,
          automaticLayout: true
        }}
      />
    </div>
  );
}
