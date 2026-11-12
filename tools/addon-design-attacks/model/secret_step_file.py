#!/usr/bin/env python3
"""A model of the design's secret rule: "No add-on can read a secret."

Each host and agent function follows one sentence of the design or the sketch, quoted above it.
The add-on functions stand in for the guest exports `steps.expand` and `steps.call`.
The agent's RunCommand runs a real shell command in a real step folder.

Finding 11. Lines 1 to 80 of fable/model/secret_step_file.py survive on #82 and are kept
as they were, apart from this paragraph. The quotes are from the first draft of the design.
`AddOn.call` and everything after it are rewritten from the finding and its output.
"""
import base64, json, os, shutil, subprocess, tempfile

SECRET = b"BULK-ENROLLMENT-TOKEN-7f3a91c2"      # the admin uploads this into a `secret_file` field
secrets = {("acme.join", "enrollment_package"): SECRET}
debug_log = []

# ------------------------------------------------------------------ host

def host_settings_get(addon):
    # host.wit: "A secret field reads as {"set": true} or {"set": false}, never as its value."
    return {"enrollment_package": {"set": (addon, "enrollment_package") in secrets}}

def host_log_write(addon, message):
    # host.wit: "The host tags the line with the add-on ID and version, caps its length, and redacts known secrets."
    for value in secrets.values():
        message = message.replace(value.decode(), "[redacted]")
    debug_log.append(f"[{addon}] {message[:200]}")

ALLOWED = {"RunCommand", "RunPowerShell", "SetVariable", "Restart", "ServerCall"}

def host_lower(addon_id, addon, config):
    # addon.wit: "The host parses it with deployarr-core and accepts only RunCommand, RunPowerShell,
    # SetVariable, Restart, and ServerCall."
    steps = json.loads(addon.expand("join", json.dumps(config)))
    for s in steps:
        assert s["type"] in ALLOWED, s["type"]
        for f in s.get("files", []):
            # core/addon.rs, FieldKind::SecretFile: "A secret file ... A lowered step can attach it."
            assert (addon_id, f["source"]["Secret"]) in secrets
    return steps

def host_server_call(addon_id, addon, config, inputs):
    # design.md: "The agent sends named variables to the server and assigns the variables that come back.
    # The server finds the add-on and config from the deployment and the node."
    return addon.call("join", json.dumps(config), inputs)

# ----------------------------------------------------------------- agent

def agent_run(addon_id, addon, config, program):
    variables = {}
    for step in program:
        if step["type"] == "RunCommand":
            # design.md: "RunCommand and RunPowerShell can list files that the agent puts in a private
            # step folder before the step and deletes after it."
            step_dir = tempfile.mkdtemp(prefix="step-")
            for f in step["files"]:
                with open(os.path.join(step_dir, f["name"]), "wb") as out:
                    out.write(secrets[(addon_id, f["source"]["Secret"])])     # "Served only to this deployment, for this step."
            var_file = os.path.join(step_dir, ".vars")
            # guest/autopilot.rs: "The script calls `deployarr-agent var set AutopilotHash <hash>`, the agent's existing channel."
            shim = f'deployarr_agent_var_set() {{ printf "%s=%s\\n" "$1" "$2" >> "{var_file}"; }}; '
            subprocess.run(["sh", "-c", shim + step["command"]], env={"_StepDir": step_dir, "PATH": "/usr/bin:/bin"}, check=True)
            if os.path.exists(var_file):
                for line in open(var_file):
                    k, v = line.rstrip("\n").split("=", 1)
                    variables[k] = v
            shutil.rmtree(step_dir)
        elif step["type"] == "ServerCall":
            host_server_call(addon_id, addon, config, {k: variables[k] for k in step["send"]})

# ---------------------------------------------------------------- add-on

class AddOn:
    received = None
    @staticmethod
    def expand(kind, config):
        return json.dumps([
            {"type": "RunCommand",
             "command": 'deployarr_agent_var_set Pkg "$(base64 -w0 "$_StepDir/enroll.ppkg")"',
             "files": [{"name": "enroll.ppkg", "source": {"Secret": "enrollment_package"}}]},
            {"type": "ServerCall", "send": ["Pkg"]},
        ])
    @staticmethod
    def call(kind, config, inputs):
        # The add-on's `steps.call` gets the variables the lowered ServerCall named.
        AddOn.received = base64.b64decode(inputs["Pkg"])
        # It logs the bytes twice: once encoded, once as text.
        host_log_write("acme.join", "debug: " + inputs["Pkg"])
        host_log_write("acme.join", "debug: " + AddOn.received.decode())
        return []

# ------------------------------------------------------------------ run

print("settings.get as the add-on sees it:", host_settings_get("acme.join"))
program = host_lower("acme.join", AddOn, {})
print("lowered program accepted by the host:", [s["type"] for s in program])
agent_run("acme.join", AddOn, {}, program)
print("bytes the add-on's `steps.call` received:", AddOn.received)
print("equal to the stored secret:", AddOn.received == SECRET)
print("debug log:", debug_log)
# The plan's check greps the debug log for the secret's text.
print("plan check, grep of the debug log for the secret finds it:", any(SECRET.decode() in line for line in debug_log))
