# Reproduce the MDT research

This page is for every agent that downloads, unpacks, or decompiles a research source. Set `SCRATCH` to a directory outside the repository, and run the scripts from the repository root.

## Sources

| Item | Location | SHA-256 |
| --- | --- | --- |
| MDT 8450 installer (6.3.8450.1000) | `https://download.microsoft.com/download/0/7/b/07b3b260-f008-4b1c-8eaa-74bcc5c7d646/MicrosoftDeploymentToolkit_x64.msi` | `e73823db601b06e9c222ae816bbddd491ecb823a1f8cce0f73eec9de51c1690e` |
| MDT 8456 hotfix KB4564442 (`Microsoft.BDD.Utility.dll` only) | `https://download.microsoft.com/download/3/0/6/306AC1B2-59BE-43B8-8C65-E141EF287A5E/KB4564442/MDT_KB4564442.exe` | `cd0bc2ec616ba5f8ddd626d02747b19c90efa1ba159303f2ff41ae693ec10b3f` |
| MDT 8450 and 8456 scripts and templates | `Source/MDT.8450` and `Source/MDT.8456` in [keithga/DeployShared](https://github.com/keithga/DeployShared) | none |
| ConfigMgr 2509 installer (`ConfigMgr_2509.exe`) | `https://go.microsoft.com/fwlink/?linkid=2350307&clcid=0x409&culture=en-us&country=us` | `4cb0380e2b1c43f1e0b2da266beddd1b975dd72537f8d8113c702eadf1e6e313` |
| WinPE add-on for ADK 10.1.26100.9457 (`adkwinpesetup.exe`) | `https://go.microsoft.com/fwlink/?linkid=2289981` | `d4de67acc83de253ccc941dd0590808d83726bdd8ea768a374a7beaff7949d6a` |

The MDT 8456 installer is gone. Its download link returned HTTP 404 on 2026-11-11. The DeployShared copy of the 8450 scripts matches the 8450 installer's scripts file for file.

## Steps

1. Install the tools. On Ubuntu 24.04:

	```bash
	apt-get install -y msitools dotnet-sdk-8.0
	```

	```bash
	dotnet tool install -g ilspycmd --version 8.2.0.7535
	```

2. Extract the installer into a scratch directory:

	```bash
	msiextract -C "$SCRATCH/mdt8450" MicrosoftDeploymentToolkit_x64.msi
	```

3. Decompile a .NET assembly to C#. ilspycmd 8.2 targets .NET 6, so it needs `DOTNET_ROLL_FORWARD=Major` to run on .NET 8:

	```bash
	DOTNET_ROLL_FORWARD=Major ilspycmd -p -o "$SCRATCH/ilspy/Microsoft.BDD.PSSnapIn" "$SCRATCH/mdt8450/Microsoft Deployment Toolkit/Bin/Microsoft.BDD.PSSnapIn.dll"
	```

4. Export a native binary's functions to pseudocode with REA and Ghidra. The output folder must not exist yet:

	```bash
	rea decompile-all "$SCRATCH/mdt8450/Microsoft Deployment Toolkit/Templates/Distribution/Tools/x64/TsCore.dll" --output "$SCRATCH/decomp/TsCore" --provider ghidra --format json
	```

5. Search the exported pseudocode with `grep`. Ghidra writes string literals inline, so searching for a log message or a variable name finds the functions that use it.

## ConfigMgr 2509

`tools/re/configmgr-osd/reproduce.sh` downloads the installer, checks its SHA-256, and unpacks it and every MSI and CAB inside it. It then writes the MDT search results, an inventory of `OSD/bin/x64`, and a catalog of every step's executable and variables. It needs `7z` (`apt-get install -y p7zip-full`), `msiextract`, and a Python with `pefile`:

```bash
python3 -m venv "$SCRATCH/venv" && "$SCRATCH/venv/bin/pip" install pefile dnfile dncil
```

```bash
PYTHON="$SCRATCH/venv/bin/python" bash tools/re/configmgr-osd/reproduce.sh "$SCRATCH/configmgr"
```

For a later release, pass the new link and SHA-256 as the second and third arguments. `ildump.py` prints the CIL of console methods that load a matching string, which is how the default disk layouts were read. It needs `dnfile` and `dncil`.

## WinPE

`tools/re/winpe/reproduce.sh` downloads the WinPE add-on setup program and checks its SHA-256. It reads the setup program's manifest and downloads one MSI. The MSI's tables name the CAB that holds the x64 `winpe.wim`, and the script downloads that CAB. It checks the SHA-1 of the MSI and the CAB against the manifest. It needs bash 4.4 or later, `curl`, `7z`, `msiinfo` from msitools, `sha1sum`, `sha256sum`, and Python 3.10 or later with no extra packages:

```bash
bash tools/re/winpe/reproduce.sh "$SCRATCH/winpe"
```

It writes four files to `"$SCRATCH/winpe/out"`:

- `winpe-x64-dlls.txt`, the module and API set list
- `system32-dlls.txt`, the modules in the case the image stores them
- `apisets.tsv`, every API set and whether it resolves
- `summary.txt`, the versions, sizes, hashes, and counts

Copy `"$SCRATCH/winpe/out/winpe-x64-dlls.txt"` to `docs/research/winpe-<version>-x64-dlls.txt`. Copy `"$SCRATCH/winpe/out/system32-dlls.txt"` to `docs/research/winpe-<version>-system32-dlls.txt`. For a later add-on, pass the new link and SHA-256 as the second and third arguments. Then write a new research page for it, and point the import check at the new list.

## Pitfalls

- Each `rea` command that uses Ghidra analyzes the binary from scratch, even with `--snapshot`. One query took about 45 to 60 seconds on `TsManager.exe`, and a full export of `TsCore.dll` took about 8 minutes. Export each binary once with `decompile-all`, then search the files.
- `rea decompile-all --resume` works only on a folder that already holds `decompile-journal.jsonl` from an earlier run.
- ILSpy cannot open native executables such as `BDDRun.exe`, `Microsoft.BDD.PnpEnum.exe`, `Microsoft.BDD.WDSEnum.exe`, and `OSDSetupWizard.exe`. Use Ghidra for those.
- The task sequence engine (`TsCore.dll`, `TsManager.exe`, `TsmBootstrap.exe`) is ConfigMgr code without symbols. Its strings include source file paths such as `tsexpreval.cpp`, which locate each subsystem.
