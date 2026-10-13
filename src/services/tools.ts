import type { Tool } from '@/types';

export type ToolRisk = 'read' | 'write' | 'exec' | 'plan';

export interface ToolMeta {
  label: string;
  risk: ToolRisk;
  /** Argument holding the primary target, shown in the tool card header. */
  target: string;
}

export const TOOL_META: Record<string, ToolMeta> = {
  list_directory: { label: 'List', risk: 'read', target: 'path' },
  read_file: { label: 'Read', risk: 'read', target: 'path' },
  search_files: { label: 'Search', risk: 'read', target: 'pattern' },
  write_file: { label: 'Write', risk: 'write', target: 'path' },
  edit_file: { label: 'Edit', risk: 'write', target: 'path' },
  delete_path: { label: 'Delete', risk: 'write', target: 'path' },
  run_command: { label: 'Run', risk: 'exec', target: 'command' },
  update_todos: { label: 'Plan', risk: 'plan', target: 'todos' },
  plan: { label: 'Plan', risk: 'plan', target: 'summary' },
};

/** Tools the agent may use while planning: nothing that reads or writes disk. */
export const PLAN_MODE_TOOLS = ['update_todos', 'plan'] as const;

export const TODO_TOOL = 'update_todos';
export const PLAN_TOOL = 'plan';

export const AGENT_TOOLS: Tool[] = [
  {
    type: 'function',
    function: {
      name: 'list_directory',
      description:
        'List files and folders in a workspace directory. Directories end with "/". Common build/dependency folders (node_modules, .git, target, dist) are skipped.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Directory relative to the workspace root. Defaults to ".".' },
          recursive: { type: 'boolean', description: 'List subdirectories recursively (up to 6 levels).' },
        },
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'read_file',
      description:
        'Read a text file from the workspace. Returns up to 2000 lines; use start_line/end_line to page through larger files.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'File path relative to the workspace root.' },
          start_line: { type: 'integer', description: '1-based first line to read.' },
          end_line: { type: 'integer', description: '1-based last line to read (inclusive).' },
        },
        required: ['path'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'search_files',
      description:
        'Search file contents with a regular expression (falls back to a literal match). Returns matching lines as path:line: text.',
      parameters: {
        type: 'object',
        properties: {
          pattern: { type: 'string', description: 'Regex or literal text to search for.' },
          path: { type: 'string', description: 'Directory to search in, relative to the workspace root. Defaults to ".".' },
          include: { type: 'string', description: 'Optional file glob filter, e.g. "*.ts" or "src/**/*.{vue,ts}".' },
          case_insensitive: { type: 'boolean', description: 'Ignore case when matching.' },
        },
        required: ['pattern'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'write_file',
      description:
        'Create a new file or completely overwrite an existing one. Parent directories are created automatically. Always provide the full file content. Prefer edit_file for small changes to existing files.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'File path relative to the workspace root.' },
          content: { type: 'string', description: 'The complete content to write.' },
        },
        required: ['path', 'content'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'edit_file',
      description:
        'Replace an exact snippet of text in an existing file. old_string must match the file exactly (including whitespace and indentation) and must be unique unless replace_all is true. Read the file first.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'File path relative to the workspace root.' },
          old_string: { type: 'string', description: 'Exact text to replace. Include enough surrounding lines to be unique.' },
          new_string: { type: 'string', description: 'Replacement text.' },
          replace_all: { type: 'boolean', description: 'Replace every occurrence instead of requiring a unique match.' },
        },
        required: ['path', 'old_string', 'new_string'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'delete_path',
      description: 'Delete a file or a directory (recursively) inside the workspace.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Path relative to the workspace root.' },
        },
        required: ['path'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'run_command',
      description:
        'Run a non-interactive shell command (PowerShell on Windows) and return its exit code and output. Use it for builds, tests, git, package managers and scripts. Do not start servers or watch processes that never exit.',
      parameters: {
        type: 'object',
        properties: {
          command: { type: 'string', description: 'The command line to execute.' },
          cwd: { type: 'string', description: 'Working directory relative to the workspace root. Defaults to the root.' },
          timeout_secs: { type: 'integer', description: 'Kill the command after this many seconds (default 120, max 900).' },
        },
        required: ['command'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'update_todos',
      description:
        'Create or update your working task list. Send the FULL list every time (not a delta); items are matched by id, so an existing id updates in place and a new id is added. Keep titles short and specific. Mark exactly one item in_progress at a time and mark items done as soon as they are finished. Use this before starting multi-step work.',
      parameters: {
        type: 'object',
        properties: {
          todos: {
            type: 'array',
            description: 'The complete task list, in order.',
            items: {
              type: 'object',
              properties: {
                id: {
                  type: 'string',
                  description: 'Stable short id used to update this item later, e.g. "explore-auth". Reuse the same id when updating.',
                },
                title: { type: 'string', description: 'Short description of the task.' },
                status: {
                  type: 'string',
                  enum: ['pending', 'in_progress', 'done'],
                  description: 'Current state. Defaults to pending.',
                },
              },
              required: ['title'],
            },
          },
        },
        required: ['todos'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'plan',
      description:
        'Record a plan for the user to approve. Use this in planning mode instead of making changes: summarise what you found, list the changes you propose, and record the task list. Nothing on disk is touched.',
      parameters: {
        type: 'object',
        properties: {
          summary: {
            type: 'string',
            description: 'Two or three sentences: what you found and the approach you propose.',
          },
          changes: {
            type: 'array',
            description: 'The specific changes you would make, one short line each, naming files.',
            items: { type: 'string' },
          },
          todos: {
            type: 'array',
            description: 'Optional task list for the work, same shape as update_todos.',
            items: {
              type: 'object',
              properties: {
                id: { type: 'string' },
                title: { type: 'string' },
                status: { type: 'string', enum: ['pending', 'in_progress', 'done'] },
              },
              required: ['title'],
            },
          },
        },
        required: ['summary'],
      },
    },
  },
];

function platformName(): string {
  const ua = navigator.userAgent;
  if (ua.includes('Windows')) return 'Windows';
  if (ua.includes('Mac')) return 'macOS';
  return 'Linux';
}

export function buildSystemPrompt(
  workspace: string | null,
  agentMode: boolean,
  planMode = false
): string {
  const os = platformName();
  const shell = os === 'Windows' ? 'PowerShell' : 'sh';
  const date = new Date().toISOString().slice(0, 10);

  if (!agentMode) {
    return `You are Kilo, a helpful expert software engineering assistant in the Kilo App desktop client. Today is ${date}. Answer clearly and concisely. Format responses in Markdown and use fenced code blocks with language tags.`;
  }

  if (planMode) {
    return `You are Kilo in PLANNING MODE in the Kilo App desktop client on ${os}. Today is ${date}.${workspace ? `\n\nWorkspace root: ${workspace}` : ''}

You are producing a plan for the user to approve. You cannot read files, edit anything or run commands in this mode — the only tools available are \`plan\` and \`update_todos\`.

# What to do
- Work from what the user has told you and what is already in the conversation. If you need to inspect the code to plan well, say exactly which files you would read and why; the user can switch to Agent mode and ask you to.
- Do not invent file contents, function names or line numbers. If you are unsure, say so and put it in the plan as something to verify.
- Keep it short. A plan that takes longer to read than the work is a bad plan.

# Then call \`plan\` once
Provide:
- **summary** — two or three sentences: what you understand the task to be and the approach you propose.
- **changes** — the specific changes you would make, one short line each, naming files.
- **todos** — the task list you would work through, in order.

After calling \`plan\`, reply with a brief restatement of the plan and stop. The user will switch to Agent mode when they want it carried out.`;
  }

  if (!workspace) {
    return `You are Kilo, an expert software engineering agent in the Kilo App desktop client on ${os}. Today is ${date}.

No workspace folder is open, so you cannot read or change files or run commands. If the user asks you to work on their code, tell them to open a project folder with the "Open folder" button, then help as best you can in the meantime. Format responses in Markdown with fenced code blocks.`;
  }

  return `You are Kilo, an expert software engineering agent running in the Kilo App desktop client on ${os}. You complete tasks by working directly in the user's project with your tools.

Workspace root: ${workspace}
Shell: ${shell}
Today: ${date}

# Keeping on track
Long, unfocused runs are the main failure mode. Avoid them:
- **Before multi-step work, call \`update_todos\`** with a short task list (3-7 items, each a concrete action). Send the whole list every time; items are matched by id.
- **Keep exactly one item \`in_progress\`** — the one you are actually doing — and mark items \`done\` the moment they are finished, not at the end.
- **Re-read your own list before starting a new step.** If you are about to do something that is not on the list, either add it or stop and ask.
- **If you find yourself repeating a step or going in circles, stop and report** what is blocking you instead of trying again. Say what you tried, what happened, and what you need.
- **Stay inside the task.** Do not refactor, rename, reformat or "improve" code the user did not ask about, even if you notice something. Mention it at the end instead.

# How to work
- Understand before changing: use list_directory, search_files and read_file to explore the relevant code. Never guess file contents.
- Make focused changes. Use edit_file for targeted edits (old_string must match the file exactly and be unique) and write_file for new files or full rewrites.
- All paths are relative to the workspace root.
- Use run_command to build, test, lint, install dependencies or use git. Commands run non-interactively in ${shell} from the workspace root. Never start dev servers or other processes that do not exit.
- Verify your work: after changing code, run the relevant build or tests when the project has them, and fix any problems you introduced.
- Follow the project's existing conventions, style and libraries.
- Call several independent read-only tools in one turn when it helps.
- If a tool call fails, read the error and adjust instead of repeating the same call.
- If the user rejects a tool call, stop and ask how they would like to proceed.
- If the user only asks a question, answer it without modifying files.

# Finishing
Mark every list item \`done\` (or say why one was skipped), then reply with a short summary: what changed, which files, and anything the user should know. Keep it to a few lines — do not restate the whole task list. Format responses in Markdown with fenced code blocks that have language tags.`;
}
