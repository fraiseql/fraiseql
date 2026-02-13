#' FraiseQL Arrow Flight Client for R
#'
#' Connect to FraiseQL Arrow Flight server and execute queries
#'
#' @examples
#' \dontrun{
#' library(arrow)
#' source("fraiseql_client.R")
#'
#' client <- connect_fraiseql("localhost", 50051)
#' df <- query_graphql(client, "{ users { id name } }")
#' print(df)
#' }

library(arrow)
library(jsonlite)

#' Connect to FraiseQL Arrow Flight server
#'
#' @param host Server hostname (default: "localhost")
#' @param port Server port (default: 50051)
#'
#' @return Flight client object
#'
#' @export
connect_fraiseql <- function(host = "localhost", port = 50051) {
  location <- paste0("grpc://", host, ":", port)
  flight_connect(location)
}

#' Execute GraphQL query
#'
#' @param client Flight client from connect_fraiseql()
#' @param query GraphQL query string
#' @param variables Optional query variables (list)
#'
#' @return data.frame with results
#'
#' @export
query_graphql <- function(client, query, variables = NULL) {
  ticket_data <- list(
    type = "GraphQLQuery",
    query = query,
    variables = variables
  )

  ticket <- toJSON(ticket_data, auto_unbox = TRUE)

  # Fetch Arrow stream
  reader <- flight_get(client, ticket)

  # Convert to R data.frame (zero-copy via Arrow)
  as.data.frame(reader$read_table())
}

#' Stream observer events
#'
#' @param client Flight client from connect_fraiseql()
#' @param entity_type Entity type to filter (e.g., "Order", "User")
#' @param start_date Start date in ISO format (optional)
#' @param end_date End date in ISO format (optional)
#' @param limit Maximum number of events (optional)
#'
#' @return data.frame with events
#'
#' @export
stream_events <- function(client, entity_type, start_date = NULL,
                          end_date = NULL, limit = NULL) {
  ticket_data <- list(
    type = "ObserverEvents",
    entity_type = entity_type,
    start_date = start_date,
    end_date = end_date,
    limit = limit
  )

  ticket <- toJSON(ticket_data, auto_unbox = TRUE)

  reader <- flight_get(client, ticket)
  as.data.frame(reader$read_table())
}

#' Stream events in batches
#'
#' @param client Flight client
#' @param entity_type Entity type to filter
#' @param batch_callback Function to call for each batch
#' @param ... Additional arguments passed to stream_events
#'
#' @export
stream_events_batched <- function(client, entity_type, batch_callback, ...) {
  ticket_data <- list(
    type = "ObserverEvents",
    entity_type = entity_type,
    ...
  )

  ticket <- toJSON(ticket_data, auto_unbox = TRUE)

  reader <- flight_get(client, ticket)

  # Process batches as they arrive
  repeat {
    batch <- reader$next()
    if (is.null(batch)) break

    df <- as.data.frame(batch)
    batch_callback(df)
  }
}

# Example usage
if (interactive()) {
  client <- connect_fraiseql()

  # Query users
  users <- query_graphql(client, "{ users { id name email } }")
  print(head(users))

  # Stream events
  events <- stream_events(client, "Order", start_date = "2026-01-01", limit = 10000)
  print(summary(events))

  # Batch processing
  process_batch <- function(df) {
    cat("Processing batch of", nrow(df), "events\n")
  }

  stream_events_batched(client, "Order", process_batch, limit = 100000)
}
