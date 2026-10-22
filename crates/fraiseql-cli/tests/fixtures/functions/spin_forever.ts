// Test fixture for `fraiseql functions invoke` (#1627).
//
// A guest that never returns: only its declared `timeout_ms` can stop it.
export default async () => {
  while (true) {}
};
