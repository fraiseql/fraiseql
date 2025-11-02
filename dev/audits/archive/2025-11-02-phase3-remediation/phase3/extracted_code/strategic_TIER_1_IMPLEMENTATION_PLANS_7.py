# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 7
def hash_audit_event(
    event_data: dict[str, Any], previous_hash: Optional[str], hash_algorithm: str = "sha256"
) -> str:
    """Generate cryptographic hash of audit event.

    Args:
        event_data: Event data (must be JSON-serializable)
        previous_hash: Previous event hash (None for first event)
        hash_algorithm: Hashing algorithm (default: sha256)

    Returns:
        Hex digest of event hash

    Raises:
        ValueError: If event_data is not JSON-serializable
    """
    if not event_data:
        raise ValueError("Event data cannot be empty")

    try:
        # Ensure deterministic ordering
        canonical_json = json.dumps(
            event_data,
            sort_keys=True,
            separators=(",", ":"),
            default=str,  # Handle UUID, datetime, etc.
        )
    except (TypeError, ValueError) as e:
        raise ValueError(f"Event data must be JSON-serializable: {e}")

    # Create chain by including previous hash
    chain_data = f"{previous_hash or 'GENESIS'}:{canonical_json}"

    # Generate hash using specified algorithm
    hasher = hashlib.new(hash_algorithm)
    hasher.update(chain_data.encode("utf-8"))

    return hasher.hexdigest()
