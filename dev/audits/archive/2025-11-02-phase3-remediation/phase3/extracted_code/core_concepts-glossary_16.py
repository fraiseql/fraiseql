# Extracted from: docs/core/concepts-glossary.md
# Block number: 16
# Traditional ORM - ALL columns loaded
class User(Base):
    id = Column(Integer)
    email = Column(String)
    password_hash = Column(String)  # Oops! Sensitive!
    api_key = Column(String)  # Oops! Sensitive!


# Easy to forget excluding fields
# One mistake = data leak
