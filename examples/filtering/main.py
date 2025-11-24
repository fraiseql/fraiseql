"""Type-Aware Filtering Example for FraiseQL.

This example demonstrates FraiseQL's automatic filter generation based on field types.
Each GraphQL type automatically gets appropriate filter operators:

- Strings: contains, startsWith, endsWith, icontains (case-insensitive)
- Numbers: gt, gte, lt, lte, between
- Booleans: eq (equality)
- Dates: before, after, between
- Arrays: contains, containedBy, overlaps
- JSONB: path queries, containment (@>)
- Network: subnet operations (<<, >>=)

No manual filter definition needed!
"""

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from enum import Enum

from fraiseql import FraiseQL

app = FraiseQL(database_url="postgresql://localhost/library")


class MembershipTier(str, Enum):
    """Library membership tiers."""

    BASIC = "basic"
    PREMIUM = "premium"
    VIP = "vip"


@app.type
@dataclass
class Book:
    """Book in the library catalog."""

    id: int
    title: str
    author: str
    isbn: str
    published_year: int
    pages: int
    price: Decimal
    genres: list[str]
    in_stock: bool
    language: str
    rating: float | None
    created_at: datetime


@app.type
@dataclass
class Member:
    """Library member."""

    id: int
    email: str
    name: str
    membership_tier: MembershipTier
    joined_date: datetime
    is_active: bool
    books_borrowed: int


# =============================================================================
# String Filtering Examples
# =============================================================================

@app.query
async def books_by_title(info, search: str, case_sensitive: bool = False) -> list[Book]:
    """Search books by title using string operators.

    Available string operators:
    - contains: Substring match
    - startsWith: Prefix match
    - endsWith: Suffix match
    - icontains: Case-insensitive substring match

    Example queries:
        ```graphql
        # Contains "Python"
        { books_by_title(search: "Python") { title author } }

        # Starts with "The"
        { books(where: { title: { startsWith: "The" } }) { title } }

        # Case-insensitive search
        { books(where: { title: { icontains: "python" } }) { title } }

        # Ends with "Guide"
        { books(where: { title: { endsWith: "Guide" } }) { title } }
        ```
    """
    db = info.context["db"]
    if case_sensitive:
        return await db.find("v_books", title__contains=search)
    return await db.find("v_books", title__icontains=search)


# =============================================================================
# Number Filtering Examples
# =============================================================================

@app.query
async def books_by_price(
    info,
    min_price: Decimal | None = None,
    max_price: Decimal | None = None,
) -> list[Book]:
    """Filter books by price range using numeric operators.

    Available numeric operators:
    - eq: Equal to
    - ne: Not equal to
    - gt: Greater than
    - gte: Greater than or equal
    - lt: Less than
    - lte: Less than or equal
    - between: Range (inclusive)

    Example queries:
        ```graphql
        # Cheap books (under $20)
        { books_by_price(max_price: 20.00) { title price } }

        # Expensive books (over $50)
        { books_by_price(min_price: 50.00) { title price } }

        # Price range $20-$40
        { books_by_price(min_price: 20.00, max_price: 40.00) { title price } }

        # Using where clause directly
        { books(where: { price: { gte: 20.00, lte: 40.00 } }) { title price } }
        ```
    """
    db = info.context["db"]
    filters = {}
    if min_price is not None:
        filters["price__gte"] = min_price
    if max_price is not None:
        filters["price__lte"] = max_price

    return await db.find("v_books", **filters)


@app.query
async def long_books(info, min_pages: int = 500) -> list[Book]:
    """Find books with many pages using comparison operators.

    Example:
        ```graphql
        # Books over 500 pages
        { long_books(min_pages: 500) { title pages } }

        # Books with exactly 300 pages
        { books(where: { pages: { eq: 300 } }) { title } }

        # Books between 200-400 pages
        { books(where: { pages: { gte: 200, lte: 400 } }) { title pages } }
        ```
    """
    db = info.context["db"]
    return await db.find("v_books", pages__gte=min_pages)


# =============================================================================
# Date/Time Filtering Examples
# =============================================================================

@app.query
async def recent_books(info, days: int = 30) -> list[Book]:
    """Find recently added books using date operators.

    Available date operators:
    - before: Earlier than
    - after: Later than
    - between: Date range

    Example queries:
        ```graphql
        # Books added in last 30 days
        { recent_books(days: 30) { title created_at } }

        # Books added after a specific date
        { books(where: { created_at: { after: "2025-01-01" } }) { title } }

        # Books added in date range
        { books(where: {
            created_at: { after: "2025-01-01", before: "2025-12-31" }
          }) { title created_at } }
        ```
    """
    db = info.context["db"]
    from datetime import timedelta

    cutoff_date = datetime.now() - timedelta(days=days)
    return await db.find("v_books", created_at__gte=cutoff_date)


# =============================================================================
# Array Filtering Examples
# =============================================================================

@app.query
async def books_by_genre(info, genres: list[str], match_all: bool = False) -> list[Book]:
    """Filter books by genres using array operators.

    Available array operators:
    - contains: Array contains all specified elements
    - containedBy: Array is contained by specified elements
    - overlaps: Array has any overlap with specified elements

    Example queries:
        ```graphql
        # Books with "Science Fiction" genre
        { books_by_genre(genres: ["Science Fiction"]) { title genres } }

        # Books with both "Mystery" AND "Thriller"
        { books_by_genre(genres: ["Mystery", "Thriller"], match_all: true) { title } }

        # Books with ANY of these genres (overlap)
        { books(where: { genres: { overlaps: ["Fantasy", "Adventure"] } }) { title } }

        # Books ONLY in these genres (contained by)
        { books(where: { genres: { containedBy: ["Fiction", "Mystery"] } }) { title } }
        ```
    """
    db = info.context["db"]
    if match_all:
        # Array contains all specified genres
        return await db.find("v_books", genres__contains=genres)
    # Array overlaps with specified genres
    return await db.find("v_books", genres__overlaps=genres)


# =============================================================================
# Boolean Filtering
# =============================================================================

@app.query
async def available_books(info, in_stock: bool = True) -> list[Book]:
    """Filter by boolean field.

    Example queries:
        ```graphql
        # In-stock books
        { available_books(in_stock: true) { title } }

        # Out-of-stock books
        { available_books(in_stock: false) { title } }

        # Using where clause
        { books(where: { in_stock: { eq: true } }) { title } }
        ```
    """
    db = info.context["db"]
    return await db.find("v_books", in_stock=in_stock)


# =============================================================================
# Enum Filtering
# =============================================================================

@app.query
async def members_by_tier(info, tier: MembershipTier) -> list[Member]:
    """Filter by enum values.

    Example queries:
        ```graphql
        # VIP members only
        { members_by_tier(tier: VIP) { name membership_tier } }

        # Premium and VIP members
        { members(where: { membership_tier: { in: [PREMIUM, VIP] } }) { name } }
        ```
    """
    db = info.context["db"]
    return await db.find("v_members", membership_tier=tier.value)


# =============================================================================
# Complex Combined Filters
# =============================================================================

@app.query
async def search_books(
    info,
    title_search: str | None = None,
    author: str | None = None,
    min_price: Decimal | None = None,
    max_price: Decimal | None = None,
    genres: list[str] | None = None,
    min_rating: float | None = None,
    in_stock: bool | None = None,
    language: str | None = None,
) -> list[Book]:
    """Combined filtering with multiple criteria.

    Demonstrates how filters can be combined with AND logic.

    Example queries:
        ```graphql
        # Complex search: Science Fiction, in stock, under $30, 4+ rating
        { search_books(
            genres: ["Science Fiction"],
            max_price: 30.00,
            min_rating: 4.0,
            in_stock: true
          ) {
            title
            author
            price
            rating
            genres
          }
        }

        # Using GraphQL where clause for complex AND/OR
        { books(where: {
            AND: [
              { price: { lte: 30.00 } },
              { rating: { gte: 4.0 } },
              { OR: [
                { genres: { contains: ["Science Fiction"] } },
                { genres: { contains: ["Fantasy"] } }
              ]}
            ]
          }) { title }
        }
        ```
    """
    db = info.context["db"]
    filters = {}

    if title_search:
        filters["title__icontains"] = title_search
    if author:
        filters["author__icontains"] = author
    if min_price is not None:
        filters["price__gte"] = min_price
    if max_price is not None:
        filters["price__lte"] = max_price
    if genres:
        filters["genres__overlaps"] = genres
    if min_rating is not None:
        filters["rating__gte"] = min_rating
    if in_stock is not None:
        filters["in_stock"] = in_stock
    if language:
        filters["language"] = language

    return await db.find("v_books", **filters)


# =============================================================================
# Database Schema
# =============================================================================
"""
-- Books table
CREATE TABLE tb_books (
    id SERIAL PRIMARY KEY,
    title VARCHAR(500) NOT NULL,
    author VARCHAR(200) NOT NULL,
    isbn VARCHAR(20) UNIQUE,
    published_year INT,
    pages INT NOT NULL,
    price DECIMAL(10,2) NOT NULL,
    genres TEXT[] NOT NULL DEFAULT '{}',
    in_stock BOOLEAN NOT NULL DEFAULT true,
    language VARCHAR(50) NOT NULL DEFAULT 'English',
    rating DECIMAL(3,2),
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for filtering performance
CREATE INDEX idx_books_title ON tb_books USING gin (to_tsvector('english', title));
CREATE INDEX idx_books_author ON tb_books USING gin (to_tsvector('english', author));
CREATE INDEX idx_books_price ON tb_books(price);
CREATE INDEX idx_books_rating ON tb_books(rating) WHERE rating IS NOT NULL;
CREATE INDEX idx_books_genres ON tb_books USING gin (genres);
CREATE INDEX idx_books_in_stock ON tb_books(in_stock) WHERE in_stock = true;
CREATE INDEX idx_books_created ON tb_books(created_at DESC);

-- Books view
CREATE VIEW v_books AS
SELECT
    id,
    title,
    author,
    isbn,
    published_year,
    pages,
    price,
    genres,
    in_stock,
    language,
    rating,
    created_at
FROM tb_books;

-- Members table
CREATE TABLE tb_members (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(200) NOT NULL,
    membership_tier VARCHAR(20) NOT NULL,
    joined_date TIMESTAMP NOT NULL DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT true,
    books_borrowed INT NOT NULL DEFAULT 0
);

CREATE VIEW v_members AS SELECT * FROM tb_members;

-- Sample data
INSERT INTO tb_books (title, author, isbn, published_year, pages, price, genres, rating) VALUES
('The Hobbit', 'J.R.R. Tolkien', '9780547928227', 1937, 310, 14.99, ARRAY['Fantasy', 'Adventure'], 4.8),
('1984', 'George Orwell', '9780451524935', 1949, 328, 15.99, ARRAY['Dystopian', 'Fiction'], 4.7),
('To Kill a Mockingbird', 'Harper Lee', '9780061120084', 1960, 324, 18.99, ARRAY['Fiction', 'Classic'], 4.8),
('The Great Gatsby', 'F. Scott Fitzgerald', '9780743273565', 1925, 180, 12.99, ARRAY['Fiction', 'Classic'], 4.4),
('Dune', 'Frank Herbert', '9780441172719', 1965, 688, 19.99, ARRAY['Science Fiction', 'Adventure'], 4.5);
"""

if __name__ == "__main__":
    import uvicorn

    from fraiseql.fastapi import create_app

    fastapi_app = create_app(app, database_url="postgresql://localhost/library")

    print("Starting FraiseQL Type-Aware Filtering Example...")
    print()
    print("This example demonstrates automatic filter generation:")
    print("  ✅ String filters: contains, startsWith, endsWith, icontains")
    print("  ✅ Numeric filters: gt, gte, lt, lte, between")
    print("  ✅ Date filters: before, after, between")
    print("  ✅ Array filters: contains, containedBy, overlaps")
    print("  ✅ Boolean filters: eq")
    print("  ✅ Complex AND/OR combinations")
    print()
    print("Open http://localhost:8000/graphql to try filtering queries")

    uvicorn.run(fastapi_app, host="0.0.0.0", port=8000)
