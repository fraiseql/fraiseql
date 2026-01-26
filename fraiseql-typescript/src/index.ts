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
 * // Export to JSON
 * if (require.main === module) {
 *   fraiseql.exportSchema("schema.json");
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
  Measure,
  Dimension,
  DimensionPath,
  DenormalizedFilter,
  FactTableDefinition,
  AggregateQueryDefinition,
  ObserverAction,
  ObserverRetryConfig,
  ObserverDefinition,
} from "./registry";

// Export decorators
export {
  field,
  Type,
  Query,
  Mutation,
  Subscription,
  FactTable as FactTableDecorator,
  AggregateQuery as AggregateQueryDecorator,
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
  FactTableConfig,
  AggregateQueryConfig,
  EnumConfig,
  InterfaceConfig,
  UnionConfig,
  InputConfig,
} from "./decorators";

// Export schema functions
export { config, exportSchema, getSchemaDict, exportSchemaToString } from "./schema";

// Export DDL generation helpers for table-backed views
export {
  loadSchema,
  generateTvDdl,
  generateTaDdl,
  generateCompositionViews,
  suggestRefreshStrategy,
  validateGeneratedDdl,
} from "./views";
export type {
  SchemaField,
  SchemaRelationship,
  SchemaType,
  SchemaObject,
  GenerateTvOptions,
  GenerateTaOptions,
  CompositionOptions,
  StrategyOptions,
} from "./views";

// Export analytics
export {
  FactTable,
  AggregateQuery,
  registerFactTableManual,
  registerTypeFieldsManual,
} from "./analytics";
export type { FactTableDecoratorConfig, AggregateQueryDecoratorConfig } from "./analytics";

// Export observers
export { Observer, webhook, slack, email, DEFAULT_RETRY_CONFIG } from "./observers";
export type {
  RetryConfig,
  WebhookAction,
  SlackAction,
  EmailAction,
  Action,
  ObserverConfig,
  ObserverDefinition as ObserverDef,
} from "./observers";

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
