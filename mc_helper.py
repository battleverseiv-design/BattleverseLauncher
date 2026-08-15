import sys
import os
import json
import argparse

# Verify that required libraries are installed, otherwise print error and exit
try:
    import minecraft_launcher_lib
except ImportError:
    print(json.dumps({"error": "minecraft-launcher-lib is not installed. Please run: pip install minecraft-launcher-lib"}))
    sys.exit(1)

def print_progress(status, progress, max_val):
    # Print progress in a clear format that Rust can read line-by-line from stdout
    print(f"STATUS:{status}|PROGRESS:{progress}|MAX:{max_val}", flush=True)

def action_install(mc_dir, mc_version, forge_version):
    try:
        # Callback for minecraft-launcher-lib installation progress
        callback = {
            "setStatus": lambda t: print_progress(t, 0, 100),
            "setProgress": lambda v: print_progress("Installing files...", v, 100),
            "setMax": lambda m: None  # minecraft_launcher_lib sometimes sends max repeatedly, we handle it natively
        }
        
        # 1. Install Forge if forge_version is provided
        if forge_version:
            forge_id = f"{mc_version}-{forge_version}"
            print_progress(f"Installing Forge {forge_id}...", 10, 100)
            minecraft_launcher_lib.forge.install_forge_version(forge_id, mc_dir, callback=callback)
        else:
            # Otherwise install vanilla
            print_progress(f"Installing Minecraft {mc_version}...", 10, 100)
            minecraft_launcher_lib.install.install_minecraft_version(mc_version, mc_dir, callback=callback)
            
        print_progress("Installation completed successfully!", 100, 100)
        print("SUCCESS", flush=True)
    except Exception as e:
        print(f"ERROR:{str(e)}", flush=True)
        sys.exit(1)

def action_get_args(mc_dir, version_id, username, ram_mb):
    try:
        # Configure launch options
        options = {
            "username": username,
            "uuid": "", # minecraft_launcher_lib generates a random uuid if empty
            "token": "",
            "jvmArguments": [f"-Xmx{ram_mb}M", f"-Xms{ram_mb}M"]
        }
        
        # Generate command arguments
        # minecraft_launcher_lib returns a dict containing executable path and arguments array
        minecraft_command = minecraft_launcher_lib.command.get_minecraft_command(version_id, mc_dir, options)
        
        # Print JSON so Rust can easily parse and execute the java executable directly
        print("COMMAND_START", flush=True)
        print(json.dumps(minecraft_command), flush=True)
        print("COMMAND_END", flush=True)
    except Exception as e:
        print(f"ERROR:{str(e)}", flush=True)
        sys.exit(1)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Minecraft Launcher Python Helper")
    subparsers = parser.add_subparsers(dest="command", required=True)
    
    # Install command
    parser_install = subparsers.add_parser("install")
    parser_install.add_argument("--dir", required=True, help="Minecraft game directory")
    parser_install.add_argument("--mc-version", required=True, help="Minecraft base version")
    parser_install.add_argument("--forge-version", default="", help="Forge version (optional)")
    
    # Get arguments command
    parser_args = subparsers.add_parser("get_args")
    parser_args.add_argument("--dir", required=True, help="Minecraft game directory")
    parser_args.add_argument("--version-id", required=True, help="Full version ID (e.g. 1.20.1-forge-47.4.10)")
    parser_args.add_argument("--username", required=True, help="Player username")
    parser_args.add_argument("--ram", type=int, required=True, help="RAM in Megabytes")
    
    args = parser.parse_args()
    
    if args.command == "install":
        action_install(args.dir, args.mc_version, args.forge_version)
    elif args.command == "get_args":
        action_get_args(args.dir, args.version_id, args.username, args.ram)
