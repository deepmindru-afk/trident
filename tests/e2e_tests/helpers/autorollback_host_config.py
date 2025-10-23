import argparse
import yaml


def add_health_checks(host_config_path):
    with open(host_config_path, "r") as f:
        host_config = yaml.safe_load(f)

    if "health" not in host_config:
        host_config["health"] = {}
    if "checks" not in host_config["health"]:
        host_config["health"]["checks"] = []

    host_config["health"]["checks"].append({})
    host_config["health"]["checks"][-1][
        "content"
    ] = "echo 'failure for ab update'\nexit 1"
    host_config["health"]["checks"][-1]["run_on"] = ["ab-update"]
    host_config["health"]["checks"][-1]["name"] = "invoke-rollback-from-script"

    host_config["health"]["checks"].append({})
    host_config["health"]["checks"][-1]["timeoutSeconds"] = 30
    host_config["health"]["checks"][-1]["systemdServices"] = [
        "non-existent-service1",
        "non-existent-service2",
    ]
    host_config["health"]["checks"][-1][
        "name"
    ] = "check-non-existent-service-to-invoke-rollback"

    with open(host_config_path, "w") as f:
        yaml.safe_dump(host_config, f)


def main():
    parser = argparse.ArgumentParser(
        description="Configures auto-rollback failure in Host Configuration."
    )
    parser.add_argument(
        "-t",
        "--hostconfig",
        type=str,
        required=True,
        help="Path to the Trident configuration file.",
    )
    args = parser.parse_args()

    add_health_checks(args.hostconfig)


if __name__ == "__main__":
    main()
