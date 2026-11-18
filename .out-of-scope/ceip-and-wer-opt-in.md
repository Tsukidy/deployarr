# CEIP and WER opt-in

deployarr has no step that opts a PC in to the Customer Experience Improvement Program (CEIP) or Windows Error Reporting (WER), as MDT's Opt In to CEIP and WER step does.

## Why this is out of scope

These choices belong to whoever finishes OOBE. In `ReadyToUse`, the unattend file's OOBE settings make them, and each client's variables can set those settings (see [Ready-to-use OOBE settings](../docs/architecture.md#ready-to-use-oobe-settings)). `ProtectYourPC` decides whether Express settings are on, and Express settings include "Send problem reports to Microsoft" ([ProtectYourPC](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-oobe-protectyourpc)). In `CustomerOobe`, the customer makes them in OOBE (see [End state](../docs/architecture.md#end-state)).

MDT's standard client task sequence holds the step in its State Restore phase, turned off (`Templates/Client.xml:381-383`). The step runs `ZTIOptIn.wsf`. It sets `HKLM\SOFTWARE\Microsoft\SQMClient\Windows\CEIPEnable` to 1 when the `CEIP` variable is `YES`, and `HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\Consent\DefaultConsent` to 3 when the `WER` variable is `YES` (`scripts/ZTIOptIn.wsf:66-83`). These paths are relative to `Source/MDT.8456` in the [MDT 8456 sources](../docs/research/mdt-8456.md#sources).

## Prior requests

- #181: "Decide what happens to the Client.xml features that deployarr has no plan for"
