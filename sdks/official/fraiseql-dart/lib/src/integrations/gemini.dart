// Google Gemini tool format integration.
// Works with both google_generative_ai and firebase_vertexai packages
// since they share the same FunctionDeclaration type.

import '../client.dart';

/// Represents a FraiseQL query as an AI tool descriptor.
/// Use this to create FunctionDeclaration objects for Gemini API calls.
class FraiseQLGeminiToolDescriptor {
  final String name;
  final String description;
  final String query;
  final Map<String, Object?> parametersSchema;
  final FraiseQLClient client;

  const FraiseQLGeminiToolDescriptor({
    required this.client,
    required this.name,
    required this.description,
    required this.query,
    required this.parametersSchema,
  });

  /// Handle a Gemini function call response and execute the corresponding query.
  ///
  /// [arguments] should be the parsed arguments from the Gemini FunctionCall.
  Future<String> execute(Map<String, Object?> arguments) async {
    final result = await client.query(query, variables: arguments);
    return result.toString();
  }
}
