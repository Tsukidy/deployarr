# BIOS block list

deployarr has no list of BIOS versions that stops a deployment, like the one behind MDT's Check BIOS step.

## Why this is out of scope

The list duplicates what deployarr already has. The `CheckReadiness` step stops a PC that fails its hardware checks before the disk changes (see [Readiness check](../docs/architecture.md#readiness-check)). Conditions on gathered facts, such as make and model, let a sequence treat a model differently, and they compare exactly (see [Core data shapes](../docs/architecture.md#core-data-shapes)). The gathered facts hold no BIOS version or release date (see [Deployment flow](../docs/architecture.md#deployment-flow)). A BIOS version fact, if wanted, is a separate request.

MDT's standard client task sequence runs Check BIOS in its Validation phase (`Templates/Client.xml:40-42`). The step runs `ZTIBIOSCheck.wsf`, which reads entries from `ZTIBIOSCheck.xml`. It fails the step when the PC's manufacturer, model, and BIOS release date all equal the values of one entry (`scripts/ZTIBIOSCheck.wsf:68-165`, `scripts/ZTIBIOSCheck.xml:17-29`). These paths are relative to `Source/MDT.8456` in the [MDT 8456 sources](../docs/research/mdt-8456.md#sources).

## Prior requests

- #181: "Decide what happens to the Client.xml features that deployarr has no plan for"
