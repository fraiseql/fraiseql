# Full-Stack Example: Go Schema → FraiseQL Backend → Flutter Mobile App

This guide demonstrates a complete end-to-end integration of a Go GraphQL schema definition with FraiseQL's compiled backend and a Flutter mobile application. We'll build a movie discovery app with searching, detailed views, reviews, and watchlist management.

**Architecture Overview:**

```
Go Schema Definition        FraiseQL Compiler        FraiseQL Server          Flutter Mobile App
(movies.go)         →      (fraiseql-cli)    →    (schema.compiled.json)  →   (Flutter client)
                                                        + Rust runtime
                                                        + PostgreSQL
```

---

## Part 1: Go Schema Authoring

### 1.1 Project Setup

Create a new Go module:

```bash
mkdir movie-schema
cd movie-schema
go mod init github.com/example/movie-schema
```

### 1.2 Go Schema Definitions

Create `models.go` with FraiseQL tags:

```go
package main

import (
 "time"
)

// Movie represents a film in the catalog
// @fraiseql.type(table="movies", cache_ttl=3600)
type Movie struct {
 ID          int       `fraiseql:"id,primary_key" json:"id"`
 Title       string    `fraiseql:"title,indexed" json:"title"`
 Description string    `fraiseql:"description" json:"description"`
 ReleaseDate time.Time `fraiseql:"release_date,indexed" json:"releaseDate"`
 Genre       string    `fraiseql:"genre,indexed" json:"genre"`
 Director    string    `fraiseql:"director" json:"director"`
 Duration    int       `fraiseql:"duration" json:"duration"` // in minutes
 PosterURL   string    `fraiseql:"poster_url" json:"posterUrl"`
 Rating      float64   `fraiseql:"rating" json:"rating"`
 ReviewCount int       `fraiseql:"review_count" json:"reviewCount"`
 CreatedAt   time.Time `fraiseql:"created_at,auto" json:"createdAt"`
 UpdatedAt   time.Time `fraiseql:"updated_at,auto" json:"updatedAt"`

 // Relations
 Reviews   []Review   `fraiseql:"reviews,foreign_key=movie_id" json:"reviews"`
 Ratings   []Rating   `fraiseql:"ratings,foreign_key=movie_id" json:"ratings"`
 Watchlist []Watchlist `fraiseql:"watchlist,foreign_key=movie_id" json:"watchlist"`
}

// Review represents a user review of a movie
// @fraiseql.type(table="reviews", cache_ttl=600)
type Review struct {
 ID        int       `fraiseql:"id,primary_key" json:"id"`
 MovieID   int       `fraiseql:"movie_id,indexed,foreign_key" json:"movieId"`
 UserID    int       `fraiseql:"user_id,indexed,foreign_key" json:"userId"`
 Username  string    `fraiseql:"username" json:"username"`
 Content   string    `fraiseql:"content" json:"content"`
 Rating    int       `fraiseql:"rating" json:"rating"` // 1-10
 Helpful   int       `fraiseql:"helpful_count" json:"helpfulCount"`
 CreatedAt time.Time `fraiseql:"created_at,auto" json:"createdAt"`

 // Relations
 Movie *Movie `fraiseql:"movie,foreign_key=movie_id" json:"movie"`
}

// Rating represents a numerical rating given by a user
// @fraiseql.type(table="ratings")
type Rating struct {
 ID        int       `fraiseql:"id,primary_key" json:"id"`
 MovieID   int       `fraiseql:"movie_id,indexed,foreign_key" json:"movieId"`
 UserID    int       `fraiseql:"user_id,indexed,foreign_key" json:"userId"`
 Score     int       `fraiseql:"score" json:"score"` // 1-10
 CreatedAt time.Time `fraiseql:"created_at,auto" json:"createdAt"`
}

// Watchlist represents a user's watchlist entry
// @fraiseql.type(table="watchlists")
type Watchlist struct {
 ID        int       `fraiseql:"id,primary_key" json:"id"`
 MovieID   int       `fraiseql:"movie_id,indexed,foreign_key" json:"movieId"`
 UserID    int       `fraiseql:"user_id,indexed,foreign_key" json:"userId"`
 Status    string    `fraiseql:"status" json:"status"` // "want_to_watch", "watching", "watched"
 AddedAt   time.Time `fraiseql:"added_at,auto" json:"addedAt"`
}

// Query represents the root query type
type Query struct{}

// Mutation represents the root mutation type
type Mutation struct{}
```

### 1.3 Query Definitions

Create `queries.go`:

```go
package main

import (
 "context"
)

// GetMovies retrieves all movies with pagination
// @fraiseql.query(name="getMovies", timeout_ms=5000)
func (q *Query) GetMovies(ctx context.Context, limit int, offset int) ([]Movie, error) {
 // This is a schema definition only - actual implementation in FraiseQL
 return nil, nil
}

// SearchMovies searches movies by title or genre
// @fraiseql.query(name="searchMovies", timeout_ms=5000)
func (q *Query) SearchMovies(ctx context.Context, query string, genre string, limit int) ([]Movie, error) {
 // Schema only
 return nil, nil
}

// GetMovieDetails retrieves a single movie with all related data
// @fraiseql.query(name="getMovieDetails", timeout_ms=5000, cache_ttl=3600)
func (q *Query) GetMovieDetails(ctx context.Context, movieID int) (*Movie, error) {
 // Schema only
 return nil, nil
}

// GetMovieReviews retrieves reviews for a specific movie
// @fraiseql.query(name="getMovieReviews", timeout_ms=5000)
func (q *Query) GetMovieReviews(ctx context.Context, movieID int, limit int, offset int) ([]Review, error) {
 // Schema only
 return nil, nil
}

// GetUserWatchlist retrieves a user's watchlist
// @fraiseql.query(name="getUserWatchlist", timeout_ms=5000)
func (q *Query) GetUserWatchlist(ctx context.Context, userID int, status string) ([]Movie, error) {
 // Schema only - status can be filtered
 return nil, nil
}

// GetTopRatedMovies retrieves highest rated movies
// @fraiseql.query(name="getTopRatedMovies", timeout_ms=5000)
func (q *Query) GetTopRatedMovies(ctx context.Context, limit int) ([]Movie, error) {
 // Schema only
 return nil, nil
}
```

### 1.4 Mutation Definitions

Create `mutations.go`:

```go
package main

// AddReview adds a new review to a movie
// @fraiseql.mutation(name="addReview", timeout_ms=3000)
type AddReviewInput struct {
 MovieID  int    `json:"movieId" validate:"required,gt=0"`
 UserID   int    `json:"userId" validate:"required,gt=0"`
 Username string `json:"username" validate:"required,min=1,max=100"`
 Content  string `json:"content" validate:"required,min=10,max=5000"`
 Rating   int    `json:"rating" validate:"required,min=1,max=10"`
}

type AddReviewOutput struct {
 ID        int    `json:"id"`
 MovieID   int    `json:"movieId"`
 UserID    int    `json:"userId"`
 Content   string `json:"content"`
 Rating    int    `json:"rating"`
 CreatedAt string `json:"createdAt"`
}

// RateMovie rates a movie
// @fraiseql.mutation(name="rateMovie", timeout_ms=3000)
type RateMovieInput struct {
 MovieID int `json:"movieId" validate:"required,gt=0"`
 UserID  int `json:"userId" validate:"required,gt=0"`
 Score   int `json:"score" validate:"required,min=1,max=10"`
}

type RateMovieOutput struct {
 ID      int `json:"id"`
 MovieID int `json:"movieId"`
 UserID  int `json:"userId"`
 Score   int `json:"score"`
}

// AddToWatchlist adds a movie to user's watchlist
// @fraiseql.mutation(name="addToWatchlist", timeout_ms=3000)
type AddToWatchlistInput struct {
 MovieID int    `json:"movieId" validate:"required,gt=0"`
 UserID  int    `json:"userId" validate:"required,gt=0"`
 Status  string `json:"status" validate:"required,oneof=want_to_watch watching watched"`
}

type AddToWatchlistOutput struct {
 ID      int    `json:"id"`
 MovieID int    `json:"movieId"`
 UserID  int    `json:"userId"`
 Status  string `json:"status"`
 AddedAt string `json:"addedAt"`
}

// UpdateWatchlistStatus updates status of a watchlist entry
// @fraiseql.mutation(name="updateWatchlistStatus", timeout_ms=3000)
type UpdateWatchlistStatusInput struct {
 WatchlistID int    `json:"watchlistId" validate:"required,gt=0"`
 Status      string `json:"status" validate:"required,oneof=want_to_watch watching watched"`
}

type UpdateWatchlistStatusOutput struct {
 ID     int    `json:"id"`
 Status string `json:"status"`
}

// RemoveFromWatchlist removes a movie from watchlist
// @fraiseql.mutation(name="removeFromWatchlist", timeout_ms=3000)
type RemoveFromWatchlistInput struct {
 WatchlistID int `json:"watchlistId" validate:"required,gt=0"`
 UserID      int `json:"userId" validate:"required,gt=0"`
}

type RemoveFromWatchlistOutput struct {
 Success bool   `json:"success"`
 Message string `json:"message"`
}
```

### 1.5 Go Module File

Create `go.mod`:

```go
module github.com/example/movie-schema

go 1.21

require (
 github.com/fraiseql/fraiseql-go v2.0.0-alpha.1
)
```

---

## Part 2: Database Schema

Create `database/schema.sql`:

```sql
-- Movies table
CREATE TABLE movies (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    description TEXT NOT NULL,
    release_date DATE NOT NULL,
    genre VARCHAR(100) NOT NULL,
    director VARCHAR(255) NOT NULL,
    duration INTEGER NOT NULL,
    poster_url VARCHAR(500) NOT NULL,
    rating DECIMAL(3, 1) DEFAULT 0.0,
    review_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_movies_genre ON movies(genre);
CREATE INDEX idx_movies_title ON movies(title);
CREATE INDEX idx_movies_release_date ON movies(release_date);

-- Reviews table
CREATE TABLE reviews (
    id SERIAL PRIMARY KEY,
    movie_id INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL,
    username VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 10),
    helpful_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_reviews_movie_id ON reviews(movie_id);
CREATE INDEX idx_reviews_user_id ON reviews(user_id);
CREATE INDEX idx_reviews_rating ON reviews(rating);

-- Ratings table
CREATE TABLE ratings (
    id SERIAL PRIMARY KEY,
    movie_id INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL,
    score INTEGER NOT NULL CHECK (score >= 1 AND score <= 10),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(movie_id, user_id)
);

CREATE INDEX idx_ratings_movie_id ON ratings(movie_id);
CREATE INDEX idx_ratings_user_id ON ratings(user_id);

-- Watchlists table
CREATE TABLE watchlists (
    id SERIAL PRIMARY KEY,
    movie_id INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL,
    status VARCHAR(50) DEFAULT 'want_to_watch' CHECK (status IN ('want_to_watch', 'watching', 'watched')),
    added_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(movie_id, user_id)
);

CREATE INDEX idx_watchlists_movie_id ON watchlists(movie_id);
CREATE INDEX idx_watchlists_user_id ON watchlists(user_id);
CREATE INDEX idx_watchlists_status ON watchlists(status);

-- Materialized view for top-rated movies
CREATE VIEW top_rated_movies AS
SELECT
    m.id,
    m.title,
    m.poster_url,
    m.rating,
    m.review_count,
    COUNT(DISTINCT r.user_id) as total_ratings
FROM movies m
LEFT JOIN ratings r ON m.id = r.movie_id
GROUP BY m.id, m.title, m.poster_url, m.rating, m.review_count
ORDER BY m.rating DESC, m.review_count DESC
LIMIT 100;

-- Function to update movie rating and review count
CREATE OR REPLACE FUNCTION update_movie_stats()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE movies
    SET
        rating = (SELECT AVG(score)::DECIMAL(3,1) FROM ratings WHERE movie_id = NEW.movie_id),
        review_count = (SELECT COUNT(*) FROM reviews WHERE movie_id = NEW.movie_id)
    WHERE id = NEW.movie_id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Triggers to keep movie stats updated
CREATE TRIGGER trigger_update_stats_on_review
AFTER INSERT ON reviews
FOR EACH ROW
EXECUTE FUNCTION update_movie_stats();

CREATE TRIGGER trigger_update_stats_on_rating
AFTER INSERT ON ratings
FOR EACH ROW
EXECUTE FUNCTION update_movie_stats();
```

---

## Part 3: Export & Compile

### 3.1 Go Export Tool

Create `cmd/export/main.go`:

```go
package main

import (
 "encoding/json"
 "flag"
 "fmt"
 "log"
 "os"
 "reflect"
 "strings"
)

type TypeDefinition struct {
 Name      string       `json:"name"`
 Table     string       `json:"table"`
 CacheTTL  int          `json:"cacheTtl,omitempty"`
 Fields    []FieldDef   `json:"fields"`
 Relations []RelationDef `json:"relations"`
}

type FieldDef struct {
 Name       string `json:"name"`
 Type       string `json:"type"`
 SQLType    string `json:"sqlType"`
 PrimaryKey bool   `json:"primaryKey"`
 Indexed    bool   `json:"indexed"`
 Auto       bool   `json:"auto"`
}

type RelationDef struct {
 Name      string `json:"name"`
 ForeignKey string `json:"foreignKey"`
 Type      string `json:"type"` // "one" or "many"
}

type QueryDefinition struct {
 Name       string `json:"name"`
 TimeoutMs  int    `json:"timeoutMs"`
 CacheTTL   int    `json:"cacheTtl,omitempty"`
 InputType  string `json:"inputType,omitempty"`
 OutputType string `json:"outputType"`
}

type MutationDefinition struct {
 Name       string `json:"name"`
 TimeoutMs  int    `json:"timeoutMs"`
 InputType  string `json:"inputType"`
 OutputType string `json:"outputType"`
}

type SchemaExport struct {
 Version   string                    `json:"version"`
 Types     map[string]TypeDefinition `json:"types"`
 Queries   []QueryDefinition         `json:"queries"`
 Mutations []MutationDefinition      `json:"mutations"`
}

func main() {
 outputFile := flag.String("o", "schema.json", "Output file for schema.json")
 flag.Parse()

 schema := SchemaExport{
  Version:   "2.0.0-alpha.1",
  Types:     make(map[string]TypeDefinition),
  Queries:   make([]QueryDefinition, 0),
  Mutations: make([]MutationDefinition, 0),
 }

 // Register types
 schema.Types["Movie"] = parseType("Movie", Movie{}, "movies", 3600)
 schema.Types["Review"] = parseType("Review", Review{}, "reviews", 600)
 schema.Types["Rating"] = parseType("Rating", Rating{}, "ratings", 0)
 schema.Types["Watchlist"] = parseType("Watchlist", Watchlist{}, "watchlists", 0)

 // Register queries
 schema.Queries = append(schema.Queries, QueryDefinition{
  Name:       "getMovies",
  TimeoutMs:  5000,
  OutputType: "[Movie]",
 })
 schema.Queries = append(schema.Queries, QueryDefinition{
  Name:       "searchMovies",
  TimeoutMs:  5000,
  OutputType: "[Movie]",
 })
 schema.Queries = append(schema.Queries, QueryDefinition{
  Name:       "getMovieDetails",
  TimeoutMs:  5000,
  CacheTTL:   3600,
  OutputType: "Movie",
 })
 schema.Queries = append(schema.Queries, QueryDefinition{
  Name:       "getMovieReviews",
  TimeoutMs:  5000,
  OutputType: "[Review]",
 })
 schema.Queries = append(schema.Queries, QueryDefinition{
  Name:       "getUserWatchlist",
  TimeoutMs:  5000,
  OutputType: "[Movie]",
 })
 schema.Queries = append(schema.Queries, QueryDefinition{
  Name:       "getTopRatedMovies",
  TimeoutMs:  5000,
  OutputType: "[Movie]",
 })

 // Register mutations
 schema.Mutations = append(schema.Mutations, MutationDefinition{
  Name:       "addReview",
  TimeoutMs:  3000,
  InputType:  "AddReviewInput",
  OutputType: "AddReviewOutput",
 })
 schema.Mutations = append(schema.Mutations, MutationDefinition{
  Name:       "rateMovie",
  TimeoutMs:  3000,
  InputType:  "RateMovieInput",
  OutputType: "RateMovieOutput",
 })
 schema.Mutations = append(schema.Mutations, MutationDefinition{
  Name:       "addToWatchlist",
  TimeoutMs:  3000,
  InputType:  "AddToWatchlistInput",
  OutputType: "AddToWatchlistOutput",
 })

 // Write schema
 data, err := json.MarshalIndent(schema, "", "  ")
 if err != nil {
  log.Fatalf("Failed to marshal schema: %v", err)
 }

 if err := os.WriteFile(*outputFile, data, 0644); err != nil {
  log.Fatalf("Failed to write schema: %v", err)
 }

 fmt.Printf("Schema exported to %s\n", *outputFile)
}

func parseType(name string, v interface{}, table string, cacheTTL int) TypeDefinition {
 t := reflect.TypeOf(v)
 def := TypeDefinition{
  Name:      name,
  Table:     table,
  CacheTTL:  cacheTTL,
  Fields:    make([]FieldDef, 0),
  Relations: make([]RelationDef, 0),
 }

 for i := 0; i < t.NumField(); i++ {
  field := t.Field(i)
  tag := field.Tag.Get("fraiseql")
  if tag == "" {
   continue
  }

  fieldDef := FieldDef{
   Name:    field.Name,
   Type:    field.Type.String(),
   SQLType: getSQLType(field.Type),
  }

  parts := strings.Split(tag, ",")
  fieldDef.Name = parts[0]
  for _, part := range parts[1:] {
   switch {
   case part == "primary_key":
    fieldDef.PrimaryKey = true
   case part == "indexed":
    fieldDef.Indexed = true
   case part == "auto":
    fieldDef.Auto = true
   }
  }

  def.Fields = append(def.Fields, fieldDef)
 }

 return def
}

func getSQLType(t reflect.Type) string {
 switch t.String() {
 case "int":
  return "INTEGER"
 case "string":
  return "VARCHAR(255)"
 case "float64":
  return "DECIMAL(3,1)"
 case "time.Time":
  return "TIMESTAMP"
 default:
  return "TEXT"
 }
}
```

### 3.2 Compilation Command

Export and compile the schema:

```bash
# From movie-schema directory
go run cmd/export/main.go -o schema.json

# Compile with fraiseql-cli
fraiseql-cli compile \
  --schema schema.json \
  --config fraiseql.toml \
  --output schema.compiled.json
```

---

## Part 4: FraiseQL Server Setup

### 4.1 FraiseQL Configuration

Create `fraiseql.toml`:

```toml
[fraiseql]
name = "movie-api"
version = "1.0.0"
description = "Movie discovery GraphQL API"

[fraiseql.server]
host = "0.0.0.0"
port = 8000
workers = 4
request_timeout_ms = 30000

[fraiseql.database]
adapter = "postgresql"
connection_string = "postgres://user:password@localhost:5432/movies"
pool_size = 20
connection_timeout_ms = 5000
query_timeout_ms = 30000

[fraiseql.database.postgresql]
ssl_mode = "prefer"
application_name = "fraiseql-movies"

[fraiseql.security]
enable_introspection = true
enable_playground = true
rate_limit_enabled = true

[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
authenticated_max_requests = 1000
authenticated_window_secs = 60

[fraiseql.security.cors]
allowed_origins = ["http://localhost:3000", "http://localhost:8081"]
allowed_methods = ["GET", "POST", "OPTIONS"]
allowed_headers = ["Content-Type", "Authorization"]
max_age_secs = 3600

[fraiseql.caching]
enabled = true
ttl_default = 300
ttl_max = 3600

[fraiseql.observability]
log_level = "info"
log_format = "json"
enable_request_logging = true
enable_query_logging = true

[fraiseql.observability.metrics]
enabled = true
export_interval_secs = 30
prometheus_enabled = true
prometheus_port = 9090
```

### 4.2 Docker Deployment

Create `Dockerfile`:

```dockerfile
# Build stage
FROM rust:1.75 as builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/fraiseql-server ./
COPY schema.compiled.json ./
COPY fraiseql.toml ./

EXPOSE 8000 9090

ENV RUST_LOG=info

CMD ["./fraiseql-server", "--config", "fraiseql.toml"]
```

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: password
      POSTGRES_DB: movies
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./database/schema.sql:/docker-entrypoint-initdb.d/schema.sql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U fraiseql"]
      interval: 10s
      timeout: 5s
      retries: 5

  fraiseql:
    build: .
    environment:
      DATABASE_URL: postgres://fraiseql:password@postgres:5432/movies
      RUST_LOG: info
    ports:
      - "8000:8000"
      - "9090:9090"
    depends_on:
      postgres:
        condition: service_healthy
    volumes:
      - ./schema.compiled.json:/app/schema.compiled.json:ro
      - ./fraiseql.toml:/app/fraiseql.toml:ro

volumes:
  postgres_data:
```

Deployment:

```bash
# Build and start services
docker-compose up --build

# Seed sample data
docker-compose exec postgres psql -U fraiseql -d movies -f /docker-entrypoint-initdb.d/seed.sql

# Check server health
curl http://localhost:8000/health
```

---

## Part 5: Flutter Mobile App

### 5.1 Flutter Project Setup

```bash
# Create Flutter project
flutter create movie_app
cd movie_app

# Add GraphQL dependencies
flutter pub add graphql
flutter pub add provider
flutter pub add http

# Install dependencies
flutter pub get
```

### 5.2 GraphQL Client Configuration

Create `lib/services/graphql_client.dart`:

```dart
import 'package:graphql/client.dart';
import 'package:http/http.dart' as http;

class GraphQLClientService {
  static final GraphQLClientService _instance = GraphQLClientService._internal();

  late GraphQLClient client;

  factory GraphQLClientService() {
    return _instance;
  }

  GraphQLClientService._internal() {
    _initializeClient();
  }

  void _initializeClient() {
    final httpLink = HttpLink(
      'http://localhost:8000/graphql',
      defaultHeaders: {
        'Content-Type': 'application/json',
      },
    );

    final authLink = AuthLink(
      getToken: () async {
        // Get auth token from secure storage
        return 'Bearer YOUR_AUTH_TOKEN';
      },
    );

    final link = Link.chain([authLink, httpLink]);

    client = GraphQLClient(
      link: link,
      cache: GraphQLCache(
        store: InMemoryStore(),
      ),
      defaultPolicies: DefaultPolicies(
        watchQuery: const Policies(
          fetch: FetchPolicy.cacheAndNetwork,
        ),
        query: const Policies(
          fetch: FetchPolicy.networkOnly,
        ),
      ),
    );
  }
}
```

### 5.3 GraphQL Queries

Create `lib/services/movie_queries.dart`:

```dart
import 'package:graphql/client.dart';

class MovieQueries {
  static final String getMovies = '''
    query GetMovies(\$limit: Int!, \$offset: Int!) {
      getMovies(limit: \$limit, offset: \$offset) {
        id
        title
        posterUrl
        genre
        rating
        releaseDate
        director
      }
    }
  ''';

  static final String searchMovies = '''
    query SearchMovies(\$query: String!, \$genre: String, \$limit: Int!) {
      searchMovies(query: \$query, genre: \$genre, limit: \$limit) {
        id
        title
        posterUrl
        genre
        rating
        director
      }
    }
  ''';

  static final String getMovieDetails = '''
    query GetMovieDetails(\$movieId: Int!) {
      getMovieDetails(movieId: \$movieId) {
        id
        title
        description
        posterUrl
        genre
        director
        duration
        rating
        releaseDate
        reviewCount
        reviews(limit: 5) {
          id
          username
          content
          rating
          createdAt
        }
      }
    }
  ''';

  static final String getUserWatchlist = '''
    query GetUserWatchlist(\$userId: Int!, \$status: String) {
      getUserWatchlist(userId: \$userId, status: \$status) {
        id
        title
        posterUrl
        genre
        rating
      }
    }
  ''';

  static final String getTopRatedMovies = '''
    query GetTopRatedMovies(\$limit: Int!) {
      getTopRatedMovies(limit: \$limit) {
        id
        title
        posterUrl
        rating
        reviewCount
      }
    }
  ''';
}

class MovieMutations {
  static final String addReview = '''
    mutation AddReview(
      \$movieId: Int!
      \$userId: Int!
      \$username: String!
      \$content: String!
      \$rating: Int!
    ) {
      addReview(
        input: {
          movieId: \$movieId
          userId: \$userId
          username: \$username
          content: \$content
          rating: \$rating
        }
      ) {
        id
        movieId
        username
        content
        rating
        createdAt
      }
    }
  ''';

  static final String addToWatchlist = '''
    mutation AddToWatchlist(
      \$movieId: Int!
      \$userId: Int!
      \$status: String!
    ) {
      addToWatchlist(
        input: {
          movieId: \$movieId
          userId: \$userId
          status: \$status
        }
      ) {
        id
        movieId
        userId
        status
        addedAt
      }
    }
  ''';

  static final String removeFromWatchlist = '''
    mutation RemoveFromWatchlist(
      \$watchlistId: Int!
      \$userId: Int!
    ) {
      removeFromWatchlist(
        input: {
          watchlistId: \$watchlistId
          userId: \$userId
        }
      ) {
        success
        message
      }
    }
  ''';
}
```

### 5.4 Models

Create `lib/models/movie.dart`:

```dart
import 'package:equatable/equatable.dart';

class Movie extends Equatable {
  final int id;
  final String title;
  final String description;
  final String posterUrl;
  final String genre;
  final String director;
  final int duration;
  final double rating;
  final int reviewCount;
  final DateTime releaseDate;
  final List<Review>? reviews;

  const Movie({
    required this.id,
    required this.title,
    required this.description,
    required this.posterUrl,
    required this.genre,
    required this.director,
    required this.duration,
    required this.rating,
    required this.reviewCount,
    required this.releaseDate,
    this.reviews,
  });

  factory Movie.fromJson(Map<String, dynamic> json) {
    return Movie(
      id: json['id'] as int,
      title: json['title'] as String,
      description: json['description'] as String,
      posterUrl: json['posterUrl'] as String,
      genre: json['genre'] as String,
      director: json['director'] as String,
      duration: json['duration'] as int,
      rating: (json['rating'] as num).toDouble(),
      reviewCount: json['reviewCount'] as int,
      releaseDate: DateTime.parse(json['releaseDate'] as String),
      reviews: (json['reviews'] as List<dynamic>?)
          ?.map((r) => Review.fromJson(r as Map<String, dynamic>))
          .toList(),
    );
  }

  @override
  List<Object?> get props => [
    id,
    title,
    description,
    posterUrl,
    genre,
    director,
    duration,
    rating,
    reviewCount,
    releaseDate,
    reviews,
  ];
}

class Review extends Equatable {
  final int id;
  final String username;
  final String content;
  final int rating;
  final DateTime createdAt;

  const Review({
    required this.id,
    required this.username,
    required this.content,
    required this.rating,
    required this.createdAt,
  });

  factory Review.fromJson(Map<String, dynamic> json) {
    return Review(
      id: json['id'] as int,
      username: json['username'] as String,
      content: json['content'] as String,
      rating: json['rating'] as int,
      createdAt: DateTime.parse(json['createdAt'] as String),
    );
  }

  @override
  List<Object?> get props => [id, username, content, rating, createdAt];
}

class WatchlistItem extends Equatable {
  final int id;
  final int movieId;
  final int userId;
  final String status;
  final DateTime addedAt;

  const WatchlistItem({
    required this.id,
    required this.movieId,
    required this.userId,
    required this.status,
    required this.addedAt,
  });

  factory WatchlistItem.fromJson(Map<String, dynamic> json) {
    return WatchlistItem(
      id: json['id'] as int,
      movieId: json['movieId'] as int,
      userId: json['userId'] as int,
      status: json['status'] as String,
      addedAt: DateTime.parse(json['addedAt'] as String),
    );
  }

  @override
  List<Object?> get props => [id, movieId, userId, status, addedAt];
}
```

### 5.5 Provider State Management

Create `lib/providers/movie_provider.dart`:

```dart
import 'package:flutter/material.dart';
import 'package:graphql/client.dart';
import '../models/movie.dart';
import '../services/graphql_client.dart';
import '../services/movie_queries.dart';

class MovieProvider extends ChangeNotifier {
  final GraphQLClient _client = GraphQLClientService().client;

  List<Movie> _movies = [];
  Movie? _selectedMovie;
  List<Review> _reviews = [];
  bool _isLoading = false;
  String? _error;

  List<Movie> get movies => _movies;
  Movie? get selectedMovie => _selectedMovie;
  List<Review> get reviews => _reviews;
  bool get isLoading => _isLoading;
  String? get error => _error;

  Future<void> fetchMovies({int limit = 20, int offset = 0}) async {
    _isLoading = true;
    _error = null;
    notifyListeners();

    try {
      final QueryOptions options = QueryOptions(
        document: gql(MovieQueries.getMovies),
        variables: {
          'limit': limit,
          'offset': offset,
        },
        fetchPolicy: FetchPolicy.networkOnly,
      );

      final QueryResult result = await _client.query(options);

      if (result.hasException) {
        _error = result.exception.toString();
        _isLoading = false;
        notifyListeners();
        return;
      }

      final List<dynamic> moviesData = result.data?['getMovies'] ?? [];
      _movies = moviesData
          .map((m) => Movie.fromJson(m as Map<String, dynamic>))
          .toList();

      _isLoading = false;
      notifyListeners();
    } catch (e) {
      _error = e.toString();
      _isLoading = false;
      notifyListeners();
    }
  }

  Future<void> searchMovies({
    required String query,
    String? genre,
    int limit = 20,
  }) async {
    _isLoading = true;
    _error = null;
    notifyListeners();

    try {
      final QueryOptions options = QueryOptions(
        document: gql(MovieQueries.searchMovies),
        variables: {
          'query': query,
          'genre': genre,
          'limit': limit,
        },
        fetchPolicy: FetchPolicy.networkOnly,
      );

      final QueryResult result = await _client.query(options);

      if (result.hasException) {
        _error = result.exception.toString();
        _isLoading = false;
        notifyListeners();
        return;
      }

      final List<dynamic> moviesData = result.data?['searchMovies'] ?? [];
      _movies = moviesData
          .map((m) => Movie.fromJson(m as Map<String, dynamic>))
          .toList();

      _isLoading = false;
      notifyListeners();
    } catch (e) {
      _error = e.toString();
      _isLoading = false;
      notifyListeners();
    }
  }

  Future<void> fetchMovieDetails(int movieId) async {
    _isLoading = true;
    _error = null;
    notifyListeners();

    try {
      final QueryOptions options = QueryOptions(
        document: gql(MovieQueries.getMovieDetails),
        variables: {'movieId': movieId},
        fetchPolicy: FetchPolicy.networkOnly,
      );

      final QueryResult result = await _client.query(options);

      if (result.hasException) {
        _error = result.exception.toString();
        _isLoading = false;
        notifyListeners();
        return;
      }

      final movieData = result.data?['getMovieDetails'] as Map<String, dynamic>;
      _selectedMovie = Movie.fromJson(movieData);
      _reviews = _selectedMovie?.reviews ?? [];

      _isLoading = false;
      notifyListeners();
    } catch (e) {
      _error = e.toString();
      _isLoading = false;
      notifyListeners();
    }
  }

  Future<bool> addReview({
    required int movieId,
    required int userId,
    required String username,
    required String content,
    required int rating,
  }) async {
    _isLoading = true;
    _error = null;
    notifyListeners();

    try {
      final MutationOptions options = MutationOptions(
        document: gql(MovieMutations.addReview),
        variables: {
          'movieId': movieId,
          'userId': userId,
          'username': username,
          'content': content,
          'rating': rating,
        },
      );

      final QueryResult result = await _client.mutate(options);

      if (result.hasException) {
        _error = result.exception.toString();
        _isLoading = false;
        notifyListeners();
        return false;
      }

      // Refresh movie details
      await fetchMovieDetails(movieId);
      return true;
    } catch (e) {
      _error = e.toString();
      _isLoading = false;
      notifyListeners();
      return false;
    }
  }

  Future<bool> addToWatchlist({
    required int movieId,
    required int userId,
    required String status,
  }) async {
    _isLoading = true;
    _error = null;
    notifyListeners();

    try {
      final MutationOptions options = MutationOptions(
        document: gql(MovieMutations.addToWatchlist),
        variables: {
          'movieId': movieId,
          'userId': userId,
          'status': status,
        },
      );

      final QueryResult result = await _client.mutate(options);

      if (result.hasException) {
        _error = result.exception.toString();
        _isLoading = false;
        notifyListeners();
        return false;
      }

      _isLoading = false;
      notifyListeners();
      return true;
    } catch (e) {
      _error = e.toString();
      _isLoading = false;
      notifyListeners();
      return false;
    }
  }
}
```

### 5.6 UI Screens

Create `lib/screens/movie_list_screen.dart`:

```dart
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../models/movie.dart';
import '../providers/movie_provider.dart';

class MovieListScreen extends StatefulWidget {
  const MovieListScreen({Key? key}) : super(key: key);

  @override
  State<MovieListScreen> createState() => _MovieListScreenState();
}

class _MovieListScreenState extends State<MovieListScreen> {
  late TextEditingController _searchController;

  @override
  void initState() {
    super.initState();
    _searchController = TextEditingController();
    // Load movies on init
    Future.microtask(() {
      context.read<MovieProvider>().fetchMovies();
    });
  }

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Movie Discovery'),
        elevation: 0,
      ),
      body: Consumer<MovieProvider>(
        builder: (context, provider, _) {
          if (provider.isLoading) {
            return const Center(child: CircularProgressIndicator());
          }

          if (provider.error != null) {
            return Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  const Icon(Icons.error_outline, size: 64, color: Colors.red),
                  const SizedBox(height: 16),
                  Text('Error: ${provider.error}'),
                  const SizedBox(height: 16),
                  ElevatedButton(
                    onPressed: () => provider.fetchMovies(),
                    child: const Text('Retry'),
                  ),
                ],
              ),
            );
          }

          return Column(
            children: [
              Padding(
                padding: const EdgeInsets.all(16.0),
                child: TextField(
                  controller: _searchController,
                  decoration: InputDecoration(
                    hintText: 'Search movies...',
                    prefixIcon: const Icon(Icons.search),
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(8),
                    ),
                  ),
                  onChanged: (value) {
                    if (value.isEmpty) {
                      provider.fetchMovies();
                    } else {
                      provider.searchMovies(query: value);
                    }
                  },
                ),
              ),
              Expanded(
                child: provider.movies.isEmpty
                    ? const Center(child: Text('No movies found'))
                    : GridView.builder(
                        padding: const EdgeInsets.all(8),
                        gridDelegate:
                            const SliverGridDelegateWithFixedCrossAxisCount(
                          crossAxisCount: 2,
                          childAspectRatio: 0.7,
                          crossAxisSpacing: 8,
                          mainAxisSpacing: 8,
                        ),
                        itemCount: provider.movies.length,
                        itemBuilder: (context, index) {
                          final movie = provider.movies[index];
                          return MovieCard(movie: movie);
                        },
                      ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class MovieCard extends StatelessWidget {
  final Movie movie;

  const MovieCard({Key? key, required this.movie}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: () {
        Navigator.push(
          context,
          MaterialPageRoute(
            builder: (context) => MovieDetailScreen(movieId: movie.id),
          ),
        );
      },
      child: Card(
        elevation: 2,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: Container(
                color: Colors.grey[300],
                child: Image.network(
                  movie.posterUrl,
                  fit: BoxFit.cover,
                  errorBuilder: (context, error, stackTrace) {
                    return Center(
                      child: Icon(
                        Icons.movie,
                        size: 48,
                        color: Colors.grey[600],
                      ),
                    );
                  },
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(8.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    movie.title,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontWeight: FontWeight.bold,
                      fontSize: 12,
                    ),
                  ),
                  const SizedBox(height: 4),
                  Row(
                    children: [
                      const Icon(Icons.star, size: 12, color: Colors.amber),
                      const SizedBox(width: 4),
                      Text(
                        '${movie.rating}',
                        style: const TextStyle(fontSize: 10),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
```

Create `lib/screens/movie_detail_screen.dart`:

```dart
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../providers/movie_provider.dart';

class MovieDetailScreen extends StatefulWidget {
  final int movieId;

  const MovieDetailScreen({Key? key, required this.movieId}) : super(key: key);

  @override
  State<MovieDetailScreen> createState() => _MovieDetailScreenState();
}

class _MovieDetailScreenState extends State<MovieDetailScreen> {
  @override
  void initState() {
    super.initState();
    Future.microtask(() {
      context.read<MovieProvider>().fetchMovieDetails(widget.movieId);
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Movie Details'),
      ),
      body: Consumer<MovieProvider>(
        builder: (context, provider, _) {
          if (provider.isLoading) {
            return const Center(child: CircularProgressIndicator());
          }

          final movie = provider.selectedMovie;
          if (movie == null) {
            return const Center(child: Text('Movie not found'));
          }

          return SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Poster
                Container(
                  color: Colors.grey[300],
                  height: 300,
                  width: double.infinity,
                  child: Image.network(
                    movie.posterUrl,
                    fit: BoxFit.cover,
                    errorBuilder: (context, error, stackTrace) {
                      return Center(
                        child: Icon(
                          Icons.movie,
                          size: 128,
                          color: Colors.grey[600],
                        ),
                      );
                    },
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // Title
                      Text(
                        movie.title,
                        style: const TextStyle(
                          fontSize: 24,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                      const SizedBox(height: 8),

                      // Rating and Info
                      Row(
                        children: [
                          const Icon(Icons.star, color: Colors.amber),
                          const SizedBox(width: 8),
                          Text('${movie.rating}/10 (${movie.reviewCount} reviews)'),
                        ],
                      ),
                      const SizedBox(height: 16),

                      // Meta info
                      Wrap(
                        spacing: 16,
                        children: [
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              const Text('Genre', style: TextStyle(fontSize: 12, color: Colors.grey)),
                              Text(movie.genre, style: const TextStyle(fontWeight: FontWeight.bold)),
                            ],
                          ),
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              const Text('Duration', style: TextStyle(fontSize: 12, color: Colors.grey)),
                              Text('${movie.duration} min', style: const TextStyle(fontWeight: FontWeight.bold)),
                            ],
                          ),
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              const Text('Director', style: TextStyle(fontSize: 12, color: Colors.grey)),
                              Text(movie.director, style: const TextStyle(fontWeight: FontWeight.bold)),
                            ],
                          ),
                        ],
                      ),
                      const SizedBox(height: 16),

                      // Description
                      Text(
                        movie.description,
                        style: const TextStyle(height: 1.6),
                      ),
                      const SizedBox(height: 24),

                      // Action buttons
                      Row(
                        children: [
                          Expanded(
                            child: ElevatedButton.icon(
                              icon: const Icon(Icons.bookmark),
                              label: const Text('Add to Watchlist'),
                              onPressed: () async {
                                final success = await provider.addToWatchlist(
                                  movieId: movie.id,
                                  userId: 1, // Replace with actual user ID
                                  status: 'want_to_watch',
                                );
                                if (success) {
                                  ScaffoldMessenger.of(context).showSnackBar(
                                    const SnackBar(
                                      content: Text('Added to watchlist!'),
                                    ),
                                  );
                                }
                              },
                            ),
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: ElevatedButton.icon(
                              icon: const Icon(Icons.rate_review),
                              label: const Text('Write Review'),
                              onPressed: () {
                                Navigator.push(
                                  context,
                                  MaterialPageRoute(
                                    builder: (context) =>
                                        ReviewFormScreen(movieId: movie.id),
                                  ),
                                );
                              },
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 24),

                      // Reviews section
                      const Text(
                        'Recent Reviews',
                        style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                      ),
                      const SizedBox(height: 12),
                      if (movie.reviews == null || movie.reviews!.isEmpty)
                        const Text('No reviews yet')
                      else
                        ListView.separated(
                          shrinkWrap: true,
                          physics: const NeverScrollableScrollPhysics(),
                          itemCount: movie.reviews!.length,
                          separatorBuilder: (context, index) => const Divider(),
                          itemBuilder: (context, index) {
                            final review = movie.reviews![index];
                            return ReviewTile(review: review);
                          },
                        ),
                    ],
                  ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}

class ReviewTile extends StatelessWidget {
  final Review;

  const ReviewTile({Key? key, required this.Review}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(
                Review.username,
                style: const TextStyle(fontWeight: FontWeight.bold),
              ),
              Row(
                children: [
                  const Icon(Icons.star, size: 14, color: Colors.amber),
                  const SizedBox(width: 4),
                  Text('${Review.rating}/10'),
                ],
              ),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            Review.content,
            style: const TextStyle(height: 1.4),
          ),
        ],
      ),
    );
  }
}
```

Create `lib/screens/review_form_screen.dart`:

```dart
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../providers/movie_provider.dart';

class ReviewFormScreen extends StatefulWidget {
  final int movieId;

  const ReviewFormScreen({Key? key, required this.movieId}) : super(key: key);

  @override
  State<ReviewFormScreen> createState() => _ReviewFormScreenState();
}

class _ReviewFormScreenState extends State<ReviewFormScreen> {
  late TextEditingController _usernameController;
  late TextEditingController _contentController;
  int _rating = 5;
  bool _isSubmitting = false;

  @override
  void initState() {
    super.initState();
    _usernameController = TextEditingController();
    _contentController = TextEditingController();
  }

  @override
  void dispose() {
    _usernameController.dispose();
    _contentController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Write Review'),
      ),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Username field
            const Text('Your Name', style: TextStyle(fontWeight: FontWeight.bold)),
            const SizedBox(height: 8),
            TextField(
              controller: _usernameController,
              decoration: InputDecoration(
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
                hintText: 'Enter your name',
              ),
            ),
            const SizedBox(height: 24),

            // Rating slider
            const Text('Rating', style: TextStyle(fontWeight: FontWeight.bold)),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: Slider(
                    value: _rating.toDouble(),
                    min: 1,
                    max: 10,
                    divisions: 9,
                    label: '$_rating/10',
                    onChanged: (value) {
                      setState(() => _rating = value.toInt());
                    },
                  ),
                ),
                Text('$_rating/10', style: const TextStyle(fontWeight: FontWeight.bold)),
              ],
            ),
            const SizedBox(height: 24),

            // Review content
            const Text('Your Review', style: TextStyle(fontWeight: FontWeight.bold)),
            const SizedBox(height: 8),
            TextField(
              controller: _contentController,
              maxLines: 8,
              decoration: InputDecoration(
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
                hintText: 'Share your thoughts about this movie...',
              ),
            ),
            const SizedBox(height: 24),

            // Submit button
            Consumer<MovieProvider>(
              builder: (context, provider, _) {
                return SizedBox(
                  width: double.infinity,
                  child: ElevatedButton(
                    onPressed: _isSubmitting
                        ? null
                        : () async {
                            if (_usernameController.text.isEmpty ||
                                _contentController.text.isEmpty) {
                              ScaffoldMessenger.of(context).showSnackBar(
                                const SnackBar(
                                  content: Text('Please fill all fields'),
                                ),
                              );
                              return;
                            }

                            setState(() => _isSubmitting = true);

                            final success = await provider.addReview(
                              movieId: widget.movieId,
                              userId: 1, // Replace with actual user ID
                              username: _usernameController.text,
                              content: _contentController.text,
                              rating: _rating,
                            );

                            setState(() => _isSubmitting = false);

                            if (success) {
                              ScaffoldMessenger.of(context).showSnackBar(
                                const SnackBar(
                                  content: Text('Review submitted successfully!'),
                                ),
                              );
                              Navigator.pop(context);
                            } else {
                              ScaffoldMessenger.of(context).showSnackBar(
                                SnackBar(
                                  content: Text('Error: ${provider.error}'),
                                ),
                              );
                            }
                          },
                    child: _isSubmitting
                        ? const SizedBox(
                            height: 20,
                            width: 20,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Text('Submit Review'),
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
```

### 5.7 Main App

Create `lib/main.dart`:

```dart
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'providers/movie_provider.dart';
import 'screens/movie_list_screen.dart';

void main() {
  runApp(const MovieApp());
}

class MovieApp extends StatelessWidget {
  const MovieApp({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => MovieProvider()),
      ],
      child: MaterialApp(
        title: 'Movie Discovery',
        theme: ThemeData(
          primarySwatch: Colors.blue,
          useMaterial3: true,
        ),
        home: const MovieListScreen(),
      ),
    );
  }
}
```

---

## Part 6: Flutter Project Structure

```
movie_app/
├── android/                      # Android native code
├── ios/                          # iOS native code
├── web/                          # Web version
├── lib/
│   ├── main.dart                # App entry point
│   ├── models/
│   │   └── movie.dart           # Movie, Review, Watchlist models
│   ├── screens/
│   │   ├── movie_list_screen.dart
│   │   ├── movie_detail_screen.dart
│   │   └── review_form_screen.dart
│   ├── providers/
│   │   └── movie_provider.dart  # State management
│   └── services/
│       ├── graphql_client.dart  # GraphQL client setup
│       └── movie_queries.dart   # GraphQL queries & mutations
├── pubspec.yaml                 # Dependencies
└── README.md
```

Create `pubspec.yaml`:

```yaml
name: movie_app
description: Movie discovery Flutter app powered by FraiseQL
publish_to: none

version: 1.0.0+1

environment:
  sdk: '>=3.0.0 <4.0.0'
  flutter: '>=3.10.0'

dependencies:
  flutter:
    sdk: flutter
  graphql: ^5.1.0
  provider: ^6.0.0
  http: ^1.1.0
  equatable: ^2.0.5
  intl: ^0.19.0

dev_dependencies:
  flutter_test:
    sdk: flutter
  flutter_lints: ^2.0.0

flutter:
  uses-material-design: true

  assets:
    - assets/images/

  fonts:
    - family: Roboto
      fonts:
        - asset: assets/fonts/Roboto-Regular.ttf
        - asset: assets/fonts/Roboto-Bold.ttf
          weight: 700
```

---

## Part 7: Running the Full Stack

### 7.1 Prerequisites

```bash
# Install required tools
# Rust/Cargo (for FraiseQL)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Flutter
git clone https://github.com/flutter/flutter.git -b stable
export PATH="$PATH:$(pwd)/flutter/bin"
flutter doctor

# Docker & Docker Compose
sudo pacman -S docker docker-compose  # Arch
# or brew install docker docker-compose  # macOS
```

### 7.2 Start FraiseQL Backend

```bash
# From fraiseql project directory
docker-compose up --build

# Verify server is running
curl http://localhost:8000/health

# In another terminal, seed sample data
docker-compose exec postgres psql -U fraiseql -d movies << 'EOF'
INSERT INTO movies (title, description, release_date, genre, director, duration, poster_url, rating) VALUES
('The Matrix', 'A computer hacker learns about the true nature of reality...', '1999-03-31', 'Sci-Fi', 'The Wachowskis', 136, 'https://image.tmdb.org/t/p/w500/f89U3ADr1oiB1mHWGEDCEvEDgJ2.jpg', 8.7),
('Inception', 'A skilled thief who steals corporate secrets through dream-sharing technology...', '2010-07-16', 'Sci-Fi', 'Christopher Nolan', 148, 'https://image.tmdb.org/t/p/w500/9gk7adHYeDMNNGceKc06f1DjQcO.jpg', 8.8),
('Interstellar', 'A team of explorers travel through a wormhole in space...', '2014-11-07', 'Sci-Fi', 'Christopher Nolan', 169, 'https://image.tmdb.org/t/p/w500/gEU2QniE6E77NI6lCU6MxlNBvIx.jpg', 8.6);
EOF
```

### 7.3 Configure Flutter App

Update `lib/services/graphql_client.dart` API endpoint:

```dart
// For development (Android Emulator)
final httpLink = HttpLink(
  'http://10.0.2.2:8000/graphql',  // Android emulator localhost
  defaultHeaders: {'Content-Type': 'application/json'},
);

// For iOS Simulator
final httpLink = HttpLink(
  'http://localhost:8000/graphql',
  defaultHeaders: {'Content-Type': 'application/json'},
);

// For physical device
final httpLink = HttpLink(
  'http://<your-machine-ip>:8000/graphql',
  defaultHeaders: {'Content-Type': 'application/json'},
);
```

### 7.4 Run Flutter App

```bash
# From movie_app directory
flutter pub get

# Run on Android Emulator
flutter run -d emulator-5554

# Run on iOS Simulator
open -a Simulator
flutter run -d sim

# Run on physical device
flutter run -d <device_id>
```

---

## Part 8: Example Workflows

### 8.1 Search for Movies

1. **User launches app** → MovieListScreen loads
2. **User types query** → TextField onChange calls `searchMovies()`
3. **FraiseQL executes query** → SQL searches title and genre fields
4. **Results display** → GridView updates with matching movies

GraphQL Query:

```graphql
query SearchMovies($query: String!, $genre: String, $limit: Int!) {
  searchMovies(query: $query, genre: $genre, limit: $limit) {
    id
    title
    posterUrl
    genre
    rating
    director
  }
}
```

### 8.2 View Movie Details with Reviews

1. **User taps movie card** → Navigate to MovieDetailScreen
2. **Screen loads** → Calls `getMovieDetails(movieId)`
3. **FraiseQL fetches all data** → Movie + related reviews in single query
4. **UI displays** → Poster, metadata, and review list

GraphQL Query:

```graphql
query GetMovieDetails($movieId: Int!) {
  getMovieDetails(movieId: $movieId) {
    id
    title
    description
    posterUrl
    genre
    director
    duration
    rating
    reviewCount
    reviews(limit: 5) {
      id
      username
      content
      rating
      createdAt
    }
  }
}
```

### 8.3 Submit a Review

1. **User taps "Write Review"** → ReviewFormScreen opens
2. **User fills form** → Name, rating (1-10), and content
3. **User submits** → Mutation sent to FraiseQL
4. **Server validates** → Input validation rules applied
5. **Database updates** → Review inserted, movie stats recalculated
6. **UI updates** → Movie details refreshed, review appears

GraphQL Mutation:

```graphql
mutation AddReview(
  $movieId: Int!
  $userId: Int!
  $username: String!
  $content: String!
  $rating: Int!
) {
  addReview(
    input: {
      movieId: $movieId
      userId: $userId
      username: $username
      content: $content
      rating: $rating
    }
  ) {
    id
    movieId
    username
    content
    rating
    createdAt
  }
}
```

### 8.4 Manage Watchlist

**Add to Watchlist:**

```graphql
mutation AddToWatchlist(
  $movieId: Int!
  $userId: Int!
  $status: String!
) {
  addToWatchlist(
    input: {
      movieId: $movieId
      userId: $userId
      status: $status
    }
  ) {
    id
    movieId
    userId
    status
    addedAt
  }
}
```

**Get Watchlist:**

```graphql
query GetUserWatchlist($userId: Int!, $status: String) {
  getUserWatchlist(userId: $userId, status: $status) {
    id
    title
    posterUrl
    genre
    rating
  }
}
```

---

## Part 9: Mobile App Deployment

### 9.1 iOS Deployment

```bash
cd ios

# Update bundle identifier in Xcode
open Runner.xcworkspace

# In Xcode:
# 1. Select Runner project
# 2. Change Bundle Identifier to com.yourcompany.movieapp
# 3. Set Team ID
# 4. Update display name

# Build for iOS
cd ..
flutter build ios --release

# Upload to App Store Connect
# Using Xcode or Transporter
open ios/Runner.xcworkspace
# Product → Scheme → Edit Scheme → Run → Release
# Product → Archive
```

### 9.2 Android Deployment

```bash
cd android

# Create keystore
keytool -genkey -v -keystore app-release-key.jks \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -alias app-key -storepass password -keypass password

cd ..

# Configure signing in android/app/build.gradle
# Add:
# signingConfigs {
#     release {
#         keyAlias = 'app-key'
#         keyPassword = 'password'
#         storeFile = file('../android/app/app-release-key.jks')
#         storePassword = 'password'
#     }
# }

# Build release APK
flutter build apk --release

# Build app bundle for Play Store
flutter build appbundle --release

# Upload to Google Play Console
# Using internal testing, closed testing, or production
```

### 9.3 App Store Release Checklist

- [ ] Update app version in `pubspec.yaml`
- [ ] Update build number
- [ ] Update CHANGELOG.md
- [ ] Test on physical devices (iOS & Android)
- [ ] Verify GraphQL queries work in production API
- [ ] Test offline handling (if applicable)
- [ ] Check image loading with actual CDN
- [ ] Verify error messages are user-friendly
- [ ] Add app store screenshots and description
- [ ] Submit for review

---

## Part 10: Troubleshooting

### API Connection Issues

**Problem**: App cannot reach FraiseQL server

- **Solution**: Verify correct IP/hostname in GraphQL client config
- For emulator: Use `10.0.2.2` instead of `localhost` on Android
- For physical device: Use machine's local IP address (`192.168.x.x`)

### GraphQL Query Errors

**Problem**: `Field does not exist` or validation errors

- **Solution**: Verify query structure matches `schema.compiled.json`
- Use GraphQL Playground: `http://localhost:8000/playground`
- Check variable types and required fields

### Database Connection Errors

**Problem**: `Connection refused` or `database does not exist`

- **Solution**:

  ```bash
  # Verify PostgreSQL is running
  docker-compose ps

  # Check logs
  docker-compose logs postgres

  # Reset database
  docker-compose down -v
  docker-compose up
  ```

### Flutter Build Issues

**Problem**: Dependency conflicts or build failures

- **Solution**:

  ```bash
  flutter clean
  flutter pub get
  flutter analyze
  flutter test
  ```

### Performance Issues

**Problem**: Slow queries or high latency

- **Solution**:
  - Enable caching in FraiseQL config
  - Check database indexes: `SELECT * FROM pg_stat_user_indexes;`
  - Monitor server metrics at `http://localhost:9090`
  - Use GraphQL query complexity analysis

### Image Loading Failures

**Problem**: Poster images not displaying

- **Solution**:
  - Verify image URLs are accessible
  - Check CORS configuration in `fraiseql.toml`
  - Enable network image caching in Flutter: `ImageCache`
  - Use alternative image service if needed

---

## Summary

This full-stack example demonstrates:

1. **Go Schema Definition**: Type-safe schema with FraiseQL tags for automatic GraphQL generation
2. **Database Layer**: PostgreSQL with optimized indexes, triggers, and views
3. **FraiseQL Compilation**: Conversion of Go types to GraphQL schema and SQL templates
4. **Rust Runtime**: High-performance compiled GraphQL execution with caching and security
5. **Flutter Mobile**: Modern, null-safe Dart with Provider state management
6. **End-to-End Integration**: Real GraphQL queries executing compiled SQL

**Key Architectural Benefits:**

- Zero-runtime overhead: Schema compiled to optimized SQL at build time
- Type safety: Go → GraphQL → Rust → Flutter, all with type validation
- Performance: Connection pooling, query caching, database indexing
- Security: Input validation, parameterized queries, rate limiting
- Developer Experience: Simple Go definitions generate complete GraphQL API

For production deployments, extend this example with authentication, error tracking, and observability tools.
