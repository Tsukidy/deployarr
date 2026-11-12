# ConfigMgr 2509 generated files

This folder holds files that `tools/re/configmgr-osd/reproduce.sh` generates from the ConfigMgr 2509 installer. The findings drawn from them are on the [ConfigMgr 2509 OSD reference](../configmgr-2509.md).

## Files

| File | Holds | Written by |
| --- | --- | --- |
| [`task-sequence-steps.md`](task-sequence-steps.md) | Each step class in `SMSSETUP/BIN/X64/_TaskSequenceProvider.mof` with its command line, and each property with its type, default, and task sequence variable | `tools/re/configmgr-osd/mofcatalog.py` |
| [`osd-x64-inventory.tsv`](osd-x64-inventory.tsv) | One row for each PE file directly under `SMSSETUP/OSD/bin/x64`, with its SHA-256, machine, native or managed kind, file and product versions, description, and link date | `tools/re/configmgr-osd/peinfo.py` |

## Rebuild them

The script needs `7z`, `msiextract`, and a Python with `pefile`, as [ConfigMgr 2509 in the agent guide](../../../AGENTS.md#configmgr-2509) lists. Run these from the repository root, with `SCRATCH` set to a directory outside the repository.

1. Make a virtual environment with the Python packages:

	```bash
	python3 -m venv "$SCRATCH/venv" && "$SCRATCH/venv/bin/pip" install pefile dnfile dncil
	```

2. Run the script:

	```bash
	PYTHON="$SCRATCH/venv/bin/python" bash tools/re/configmgr-osd/reproduce.sh "$SCRATCH/configmgr"
	```

3. Copy `"$SCRATCH/configmgr/out/task-sequence-steps.md"` and `"$SCRATCH/configmgr/out/osd-x64-inventory.tsv"` into this folder.

The script also writes `mdt-markers.txt`, the MDT search results. This folder does not keep that file.
