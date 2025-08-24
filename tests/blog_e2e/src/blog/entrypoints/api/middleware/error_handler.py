"""Error handling middleware for Blog Demo Application.

Provides comprehensive error handling with proper logging, monitoring,
and user-friendly error responses.
"""

import logging
import traceback
import time
from typing import Any, Dict

from fastapi import Request, HTTPException
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse

from ....config import config
from ....core.exceptions import (
    BlogException,
    BlogValidationError,
    BlogNotFoundError,
    BlogDuplicateError,
    BlogAuthorizationError,
    BlogBusinessLogicError
)


logger = logging.getLogger(__name__)


class ErrorHandlerMiddleware(BaseHTTPMiddleware):
    """Comprehensive error handling middleware."""

    def __init__(self, app):
        super().__init__(app)
        self.debug = config.debug
        self.environment = config.environment

    async def dispatch(self, request: Request, call_next):
        """Handle requests with comprehensive error catching."""

        start_time = time.time()
        request_id = getattr(request.state, "request_id", "unknown")

        try:
            response = await call_next(request)
            return response

        except HTTPException as e:
            # FastAPI HTTPExceptions - pass through with logging
            logger.warning(
                f"HTTP exception in request {request_id}: {e.status_code} - {e.detail}",
                extra={
                    "request_id": request_id,
                    "status_code": e.status_code,
                    "detail": e.detail,
                    "path": str(request.url.path),
                    "method": request.method,
                    "client_ip": getattr(request.state, "client_ip", "unknown")
                }
            )
            raise

        except BlogValidationError as e:
            # Validation errors - 400 Bad Request
            logger.info(
                f"Validation error in request {request_id}: {e.message}",
                extra={
                    "request_id": request_id,
                    "error_code": e.code,
                    "field": e.field,
                    "value": str(e.value) if e.value else None
                }
            )

            return JSONResponse(
                status_code=400,
                content={
                    "error": "Validation Error",
                    "message": e.message,
                    "code": e.code,
                    "details": {
                        "field": e.field,
                        "value": str(e.value) if e.value else None,
                        **e.details
                    },
                    "request_id": request_id
                }
            )

        except BlogNotFoundError as e:
            # Not found errors - 404 Not Found
            logger.info(
                f"Not found error in request {request_id}: {e.message}",
                extra={
                    "request_id": request_id,
                    "entity_type": e.entity_type,
                    "identifier": str(e.identifier)
                }
            )

            return JSONResponse(
                status_code=404,
                content={
                    "error": "Not Found",
                    "message": e.message,
                    "code": e.code,
                    "details": {
                        "entity_type": e.entity_type,
                        "identifier": str(e.identifier),
                        **e.details
                    },
                    "request_id": request_id
                }
            )

        except BlogDuplicateError as e:
            # Duplicate errors - 409 Conflict
            logger.info(
                f"Duplicate error in request {request_id}: {e.message}",
                extra={
                    "request_id": request_id,
                    "entity_type": e.entity_type,
                    "field": e.field,
                    "value": str(e.value)
                }
            )

            return JSONResponse(
                status_code=409,
                content={
                    "error": "Conflict",
                    "message": e.message,
                    "code": e.code,
                    "details": {
                        "entity_type": e.entity_type,
                        "field": e.field,
                        "value": str(e.value),
                        "existing_id": str(e.existing_id) if e.existing_id else None,
                        **e.details
                    },
                    "request_id": request_id
                }
            )

        except BlogAuthorizationError as e:
            # Authorization errors - 403 Forbidden
            logger.warning(
                f"Authorization error in request {request_id}: {e.message}",
                extra={
                    "request_id": request_id,
                    "action": e.action,
                    "resource": e.resource,
                    "user_id": str(e.user_id) if e.user_id else None
                }
            )

            return JSONResponse(
                status_code=403,
                content={
                    "error": "Forbidden",
                    "message": e.message,
                    "code": e.code,
                    "details": {
                        "action": e.action,
                        "resource": e.resource,
                        **e.details
                    } if self.debug else {},
                    "request_id": request_id
                }
            )

        except BlogBusinessLogicError as e:
            # Business logic errors - 422 Unprocessable Entity
            logger.info(
                f"Business logic error in request {request_id}: {e.message}",
                extra={
                    "request_id": request_id,
                    "constraint": e.constraint
                }
            )

            return JSONResponse(
                status_code=422,
                content={
                    "error": "Business Logic Error",
                    "message": e.message,
                    "code": e.code,
                    "details": {
                        "constraint": e.constraint,
                        **e.details
                    },
                    "request_id": request_id
                }
            )

        except BlogException as e:
            # Generic blog errors - 400 Bad Request
            logger.error(
                f"Blog error in request {request_id}: {e.message}",
                extra={
                    "request_id": request_id,
                    "error_code": e.code,
                    "details": e.details
                }
            )

            return JSONResponse(
                status_code=400,
                content={
                    "error": "Application Error",
                    "message": e.message,
                    "code": e.code,
                    "details": e.details if self.debug else {},
                    "request_id": request_id
                }
            )

        except Exception as e:
            # Unexpected errors - 500 Internal Server Error
            process_time = time.time() - start_time

            logger.error(
                f"Unexpected error in request {request_id}: {str(e)}",
                extra={
                    "request_id": request_id,
                    "exception_type": type(e).__name__,
                    "path": str(request.url.path),
                    "method": request.method,
                    "process_time": process_time,
                    "client_ip": getattr(request.state, "client_ip", "unknown"),
                    "user_agent": getattr(request.state, "user_agent", "unknown"),
                    "traceback": traceback.format_exc() if self.debug else None
                }
            )

            # Prepare error response
            error_content = {
                "error": "Internal Server Error",
                "message": "An unexpected error occurred. Please try again later.",
                "code": "INTERNAL_ERROR",
                "request_id": request_id
            }

            # Add debug information in development
            if self.debug:
                error_content["details"] = {
                    "exception_type": type(e).__name__,
                    "exception_message": str(e),
                    "traceback": traceback.format_exc().split("\n")
                }

            return JSONResponse(
                status_code=500,
                content=error_content
            )


class GraphQLErrorFormatter:
    """Format GraphQL errors according to the specification."""

    @staticmethod
    def format_error(error: Exception, debug: bool = False) -> Dict[str, Any]:
        """Format error for GraphQL response."""

        if isinstance(error, BlogValidationError):
            return {
                "message": error.message,
                "extensions": {
                    "code": error.code,
                    "field": error.field,
                    "value": str(error.value) if error.value else None,
                    "details": error.details
                }
            }

        elif isinstance(error, BlogNotFoundError):
            return {
                "message": error.message,
                "extensions": {
                    "code": error.code,
                    "entity_type": error.entity_type,
                    "identifier": str(error.identifier),
                    "details": error.details
                }
            }

        elif isinstance(error, BlogDuplicateError):
            return {
                "message": error.message,
                "extensions": {
                    "code": error.code,
                    "entity_type": error.entity_type,
                    "field": error.field,
                    "value": str(error.value),
                    "existing_id": str(error.existing_id) if error.existing_id else None,
                    "details": error.details
                }
            }

        elif isinstance(error, BlogAuthorizationError):
            return {
                "message": error.message,
                "extensions": {
                    "code": error.code,
                    "action": error.action,
                    "resource": error.resource,
                    "details": error.details if debug else {}
                }
            }

        elif isinstance(error, BlogBusinessLogicError):
            return {
                "message": error.message,
                "extensions": {
                    "code": error.code,
                    "constraint": error.constraint,
                    "details": error.details
                }
            }

        elif isinstance(error, BlogException):
            return {
                "message": error.message,
                "extensions": {
                    "code": error.code,
                    "details": error.details if debug else {}
                }
            }

        else:
            # Unexpected error
            logger.error(f"Unexpected GraphQL error: {str(error)}", exc_info=True)

            return {
                "message": "An unexpected error occurred" if not debug else str(error),
                "extensions": {
                    "code": "INTERNAL_ERROR",
                    "exception_type": type(error).__name__ if debug else None,
                    "traceback": traceback.format_exc().split("\n") if debug else None
                }
            }
