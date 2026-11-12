# Web client (deferred)

React + TypeScript, React Flow, Tailwind CSS, shadcn/ui and @monaco-editor/react.
The GUI and admin dashboard consume the hosted API. No frontend package/toolchain
is initialized yet; implementation follows backend acceptance.

V1 canvas changes presentation/layout only. Engineering edits go through requirements.
Monaco edits JSON/YAML inputs and catalog drafts; published artifacts are read-only.
Below 768 CSS px, workspace and admin display: "Designed for desktop, laptop, and
tablet workspaces. Please open on a larger screen".

See PRD FR-17–FR-22 and plan.md for complete requirements. Visual tokens, layout
library and bundler are not selected by this scaffold.
