# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 6
# Generate parent table
continent_rows = generator.generate_rows("catalog.tb_continent", count=7)
continent_pks = generator.insert_generated_data("catalog.tb_continent", continent_rows)

# Generate child table - FKs automatically resolve to parent integers
country_rows = generator.generate_rows("catalog.tb_country", count=50)
# Each row's fk_continent will be an integer from continent_pks
country_pks = generator.insert_generated_data("catalog.tb_country", country_rows)
