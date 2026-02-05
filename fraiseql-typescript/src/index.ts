/**
 * FraiseQL v2 - TypeScript Schema Authoring.
 *
 * This module provides decorators for defining GraphQL schemas that are compiled
 * by the FraiseQL Rust engine. NO runtime FFI - decorators output JSON only.
 *
 * Architecture:
 *   TypeScript @decorators → schema.json → fraiseql-cli → schema.compiled.json → Rust runtime
 *
 * @example
 * ```typescript
 * import * as fraiseql from "fraiseql";
 *
 * @fraiseql.Type()
 * class User {
 *   id: number;
 *   name: string;
 *   email: string;
 * }
 *
 * @fraiseql.Query({ sqlSource: "v_user" })
 * function users(limit: number = 10): User[] {
 *   // Function body not executed - only for type/metadata
 * }
 *
 * // Export minimal types.json (use fraiseql.toml for queries, security, etc.)
 * if (require.main === module) {
 *   fraiseql.exportTypes("types.json");
 * }
 * ```
 *
 * @packageDocumentation
 */

export const version = "2.0.0-alpha.1";

// Export type system
export { typeToGraphQL, extractFieldInfo, extractFunctionSignature } from "./types";
export type { FieldInfo, ArgumentInfo, ReturnTypeInfo, FunctionSignature } from "./types";

// Export registry
export { SchemaRegistry } from "./registry";
export type {
  Field,
  FieldMetadata,
  TypeDefinition,
  QueryDefinition,
  MutationDefinition,
  SubscriptionDefinition,
  ArgumentDefinition,
  EnumValue,
  EnumDefinition,
  InterfaceDefinition,
  InputTypeDefinition,
  UnionDefinition,
  Schema,
} from "./registry";

// Export decorators
export {
  field,
  Type,
  Query,
  Mutation,
  Subscription,
  enum_,
  interface_,
  union,
  input,
  registerTypeFields,
  registerQuery,
  registerMutation,
  registerSubscription,
} from "./decorators";
export type {
  TypeConfig,
  OperationConfig,
  MutationConfig,
  SubscriptionConfig,
  EnumConfig,
  InterfaceConfig,
  UnionConfig,
  InputConfig,
} from "./decorators";

// Export schema functions
export { config, exportSchema, exportTypes, getSchemaDict, exportSchemaToString } from "./schema";

// Export scalar types for schema authoring
export {
  // Core scalars
  SCALAR_NAMES,
  isScalarType,
} from "./scalars";
export type {
  // Core scalars
  ID,
  UUID,
  Json,
  Decimal,
  Vector,
  // Date/Time scalars
  DateTime,
  Date,
  Time,
  DateRange,
  Duration,
  // Contact/Communication scalars
  Email,
  PhoneNumber,
  URL,
  DomainName,
  Hostname,
  // Location/Address scalars
  PostalCode,
  Latitude,
  Longitude,
  Coordinates,
  Timezone,
  LocaleCode,
  LanguageCode,
  CountryCode,
  // Financial scalars
  IBAN,
  CUSIP,
  ISIN,
  SEDOL,
  LEI,
  MIC,
  CurrencyCode,
  Money,
  ExchangeCode,
  ExchangeRate,
  StockSymbol,
  Percentage,
  // Identifier scalars
  Slug,
  SemanticVersion,
  HashSHA256,
  APIKey,
  LicensePlate,
  VIN,
  TrackingNumber,
  ContainerNumber,
  // Networking scalars
  IPAddress,
  IPv4,
  IPv6,
  MACAddress,
  CIDR,
  Port,
  // Transportation scalars
  AirportCode,
  PortCode,
  FlightNumber,
  // Content scalars
  Markdown,
  HTML,
  MimeType,
  Color,
  Image,
  File,
  // Database scalars
  LTree,
} from "./scalars";
