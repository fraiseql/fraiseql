# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 4
# src/fraiseql/mutations/mutation_decorator.py


def create_resolver(self) -> Callable:
    """Create the GraphQL resolver function."""

    async def resolver(info, input):
        """Auto-generated resolver for PostgreSQL mutation."""
        # Get database connection
        db = info.context.get("db")
        if not db:
            msg = "No database connection in context"
            raise RuntimeError(msg)

        # Convert input to dict
        input_data = _to_dict(input)

        # Call prepare_input hook if defined
        if hasattr(self.mutation_class, "prepare_input"):
            input_data = self.mutation_class.prepare_input(input_data)

        # Build function name
        full_function_name = f"{self.schema}.{self.function_name}"

        # 🚀 ULTRA-DIRECT PATH: Use raw JSON execution
        # Check if db supports raw JSON execution
        if hasattr(db, "execute_function_raw_json"):
            logger.debug(f"Using ultra-direct mutation path for {full_function_name}")

            # Determine type name (use success type for transformer)
            type_name = self.success_type.__name__ if self.success_type else None

            try:
                # Execute with raw JSON (no parsing!)
                raw_result = await db.execute_function_raw_json(
                    full_function_name, input_data, type_name=type_name
                )

                # Return RawJSONResult directly
                # FastAPI will recognize this and return it without serialization
                logger.debug(f"✅ Ultra-direct mutation completed: {full_function_name}")
                return raw_result

            except Exception as e:
                logger.warning(
                    f"Ultra-direct mutation path failed: {e}, falling back to standard path"
                )
                # Fall through to standard path

        # 🐌 FALLBACK: Standard path (parsing + serialization)
        logger.debug(f"Using standard mutation path for {full_function_name}")

        if self.context_params:
            # ... existing context handling ...
            result = await db.execute_function_with_context(
                full_function_name,
                context_args,
                input_data,
            )
        else:
            result = await db.execute_function(full_function_name, input_data)

        # Parse result into Success or Error type
        parsed_result = parse_mutation_result(
            result,
            self.success_type,
            self.error_type,
            self.error_config,
        )

        return parsed_result

    # ... rest of resolver setup ...
    return resolver
