# Deployment tattoo

deployarr does not record a deployment's details on the PC, as MDT's Tattoo step does.

## Why this is out of scope

The tattoo is not needed. The server keeps a record of every deployment, with its own ID and its events (see [Server](../docs/architecture.md#server)). For `CustomerOobe`, the agent removes its service, and the PC keeps no account or password that deployarr created (see [End state](../docs/architecture.md#end-state)). A tattoo would leave deployarr data on a PC that goes to a customer. An admin who wants a marker on the PC can add a `RunPowerShell` step that writes one.

MDT's standard client task sequence runs Tattoo in its State Restore phase (`Templates/Client.xml:354, 378-380`). The step runs `ZTITatoo.wsf`, which records the deployment method, type, source, and time, the toolkit version, and the task sequence's ID, name, and version in the WMI class `Microsoft_BDD_Info` (`scripts/ZTITatoo.wsf:86-111, 173-211`). That class keeps each value under `HKLM\Software\Microsoft\Deployment 4` (`scripts/ZTITatoo.mof:53-166`). These paths are relative to `Source/MDT.8456` in the [MDT 8456 sources](../docs/research/mdt-8456.md#sources).

## Prior requests

- #181: "Decide what happens to the Client.xml features that deployarr has no plan for"
