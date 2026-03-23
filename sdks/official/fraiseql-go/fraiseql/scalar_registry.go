package fraiseql

import (
	"fmt"
	"sync"
)

// customScalarRegistry is the global registry for custom scalars.
type customScalarRegistry struct {
	mu      sync.RWMutex
	scalars map[string]CustomScalar
}

// Global instance
var scalarRegistry = &customScalarRegistry{
	scalars: make(map[string]CustomScalar),
}

// RegisterCustomScalar registers a custom scalar with the global registry.
//
// The scalar must have a non-empty name returned by Name().
//
// Example:
//
//	type Email struct{}
//
//	func (e *Email) Name() string { return "Email" }
//	// ... implement other methods ...
//
//	func init() {
//	    RegisterCustomScalar(&Email{})
//	}
//
// Panics if a scalar with the same name is already registered.
func RegisterCustomScalar(scalar CustomScalar) {
	name := scalar.Name()
	if name == "" {
		panic("CustomScalar must have a non-empty name")
	}

	scalarRegistry.mu.Lock()
	defer scalarRegistry.mu.Unlock()

	if _, exists := scalarRegistry.scalars[name]; exists {
		panic(fmt.Sprintf("Scalar \"%s\" is already registered", name))
	}

	scalarRegistry.scalars[name] = scalar
}

// GetCustomScalar retrieves a registered custom scalar by name.
//
// Returns nil if the scalar is not registered.
func GetCustomScalar(name string) CustomScalar {
	scalarRegistry.mu.RLock()
	defer scalarRegistry.mu.RUnlock()
	return scalarRegistry.scalars[name]
}

// GetAllCustomScalars returns all registered custom scalars.
//
// Returns a map of scalar names to scalar implementations.
func GetAllCustomScalars() map[string]CustomScalar {
	scalarRegistry.mu.RLock()
	defer scalarRegistry.mu.RUnlock()

	// Return a copy to prevent external modifications
	result := make(map[string]CustomScalar)
	for name, scalar := range scalarRegistry.scalars {
		result[name] = scalar
	}
	return result
}

// HasCustomScalar checks if a custom scalar is registered.
func HasCustomScalar(name string) bool {
	scalarRegistry.mu.RLock()
	defer scalarRegistry.mu.RUnlock()
	_, exists := scalarRegistry.scalars[name]
	return exists
}

// UnregisterCustomScalar unregisters a custom scalar (useful for testing).
func UnregisterCustomScalar(name string) {
	scalarRegistry.mu.Lock()
	defer scalarRegistry.mu.Unlock()
	delete(scalarRegistry.scalars, name)
}

// ClearCustomScalars clears all registered custom scalars (useful for testing).
func ClearCustomScalars() {
	scalarRegistry.mu.Lock()
	defer scalarRegistry.mu.Unlock()
	scalarRegistry.scalars = make(map[string]CustomScalar)
}
