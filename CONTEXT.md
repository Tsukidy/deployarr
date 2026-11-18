# deployarr

deployarr is a self-hosted Windows deployment server modeled on MDT. This glossary names the concepts of a task sequence and of the path from a saved sequence to what the agent runs. [The architecture](docs/architecture.md) holds the design and the reasons for it.

## Language

### Task sequences

**Authored sequence**:
A task sequence as an admin edits it and the server stores it. Its steps can hold built-in actions, add-on steps, and join slots. See [Task sequences stay loadable](docs/architecture.md#task-sequences-stay-loadable).
_Avoid_: stored sequence

**Lowered sequence**:
The stage inside planning at which each deferred step holds its expansion, so every step holds a built-in action. Only a lowered sequence can become a program, and it never leaves planning. See [Task sequences stay loadable](docs/architecture.md#task-sequences-stay-loadable).

**Add-on step**:
A step whose work an add-on supplies. It is plain data that names the add-on, a step kind, and the step's config, so it still loads and saves when its add-on is disabled or removed. See [Task sequences stay loadable](docs/architecture.md#task-sequences-stay-loadable).
_Avoid_: step from an add-on

**Join slot**:
A step that marks where the deployment's join profile adds its steps. Each join slot names one stage, the offline image in WinPE or the full OS. See [Directory join](docs/architecture.md#directory-join).

**Deferred step**:
An add-on step or a join slot that the program keeps, so planning must expand it. A disabled one, or one inside a disabled group, is not deferred. See [Task sequences stay loadable](docs/architecture.md#task-sequences-stay-loadable).

**Expansion**:
The steps that take one deferred step's place when a deployment is planned. They are the steps that an add-on returns for it, or none, as for a join slot whose profile joins a workgroup or a domain. See [Task sequences stay loadable](docs/architecture.md#task-sequences-stay-loadable).

### From save to program

**Save check**:
The checks that run each time an admin saves an authored sequence. It runs no add-on code, so it checks each deferred step as an unexpanded deferred step. See [Compile the tree into a flat program](docs/architecture.md#compile-the-tree-into-a-flat-program).
_Avoid_: validation

**Unexpanded deferred step**:
A deferred step as the save check sees it, before any add-on has expanded it. Its expansion may hold any action that an add-on may return, so the save check reports a rule about program order only when every such expansion would break it. See [Compile the tree into a flat program](docs/architecture.md#compile-the-tree-into-a-flat-program).
_Avoid_: slot, placeholder, stand-in

**Planning**:
The work that prepares an authored sequence for one deployment before its first step runs. Each deferred step gets its expansion, which makes a lowered sequence. Planning checks it against the same rules as the save check, then turns it into a program. See [Task sequences stay loadable](docs/architecture.md#task-sequences-stay-loadable).
_Avoid_: the plan, which also names what the add-on lifecycle planner makes

**Program**:
The flat list of instructions that planning builds from a lowered sequence. The agent runs it from start to end without the tree, and a running deployment keeps the program that planning froze for it. See [Compile the tree into a flat program](docs/architecture.md#compile-the-tree-into-a-flat-program).
