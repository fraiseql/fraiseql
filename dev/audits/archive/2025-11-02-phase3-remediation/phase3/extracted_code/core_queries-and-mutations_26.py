# Extracted from: docs/core/queries-and-mutations.md
# Block number: 26
from fraiseql import input, mutation


@input
class NetworkConfigInput:
    ip_address: str
    subnet_mask: str


@mutation
class CreateNetworkConfig:
    input: NetworkConfigInput
    success: NetworkConfigSuccess
    failure: NetworkConfigError

    @staticmethod
    def prepare_input(input_data: dict) -> dict:
        """Transform IP + subnet mask to CIDR notation."""
        ip = input_data.get("ip_address")
        mask = input_data.get("subnet_mask")

        if ip and mask:
            # Convert subnet mask to CIDR prefix
            cidr_prefix = {
                "255.255.255.0": 24,
                "255.255.0.0": 16,
                "255.0.0.0": 8,
            }.get(mask, 32)

            return {
                "ip_address": f"{ip}/{cidr_prefix}",
                # subnet_mask field is removed
            }
        return input_data


# Frontend sends: { ipAddress: "192.168.1.1", subnetMask: "255.255.255.0" }
# Database receives: { ip_address: "192.168.1.1/24" }
