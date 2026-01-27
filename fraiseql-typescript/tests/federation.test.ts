/**
 * Federation decorator tests for Cycle 16-3 RED phase.
 *
 * Tests TypeScript federation decorators: @Key, @Extends, @External, @Requires, @Provides.
 * All tests expected to FAIL initially (RED phase).
 */

import {
    Type,
    Key,
    Extends,
    External,
    Requires,
    Provides,
    ID,
    generateSchemaJson,
    validateFederation,
    SchemaRegistry,
} from '../src/federation';

describe('TypeScript Federation Decorators', () => {
    beforeEach(() => {
        // Clear the global schema registry to prevent cross-test contamination
        // SchemaRegistry is a singleton, so it accumulates types from all tests
        SchemaRegistry.clear();
    });

    describe('@Key decorator', () => {
        it('marks type with federation key', () => {
            @Key('id')
            @Type()
            class User {
                id: string;
                email: string;
            }

            expect(User.__fraiseqlFederation__.keys).toEqual([{ fields: ['id'] }]);
        });

        it('supports multiple keys', () => {
            @Key('tenant_id')
            @Key('id')
            @Type()
            class Account {
                tenant_id: string;
                id: string;
                name: string;
            }

            const keys = Account.__fraiseqlFederation__.keys;
            expect(keys).toHaveLength(2);
            expect(keys).toContainEqual({ fields: ['tenant_id'] });
            expect(keys).toContainEqual({ fields: ['id'] });
        });

        it('rejects non-existent fields', () => {
            @Key('nonexistent')
            @Type()
            class User {
                id: string;
            }

            expect(() => {
                validateFederation([User]);
            }).toThrow('Field \'nonexistent\' not found');
        });

        it('requires @Type decorator', () => {
            @Key('id')
            class User {
                id: string;
            }

            expect(() => {
                validateFederation([User]);
            }).toThrow('@Key requires @Type decorator');
        });
    });

    describe('@Extends decorator', () => {
        it('marks type as extended', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
            }

            expect(User.__fraiseqlFederation__.extend).toBe(true);
        });

        it('requires @Key decorator', () => {
            expect(() => {
                @Extends()
                @Type()
                class User {
                    id: string;
                }
            }).toThrow();
        });

        it('works with @External decorator', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;
                orders: string[];  // Regular field
            }

            expect(User.__fraiseqlFederation__.extend).toBe(true);
            expect(User.__fraiseqlFederation__.external_fields).toContain('id');
            expect(User.__fraiseqlFederation__.external_fields).toContain('email');
            expect(User.__fraiseqlFederation__.external_fields).not.toContain('orders');
        });
    });

    describe('@External decorator', () => {
        it('marks field as external', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;
            }

            expect(User.__fraiseqlFederation__.external_fields).toContain('id');
            expect(User.__fraiseqlFederation__.external_fields).toContain('email');
        });

        it('cannot be used without @Extends', () => {
            @Type()
            class User {
                @External() id: string;
            }

            expect(() => {
                validateFederation([User]);
            }).toThrow('@external requires @extends');
        });

        it('cannot be used on non-field context', () => {
            expect(() => {
                class User {
                    @External()
                    method() {
                    }
                }
            }).toThrow();
        });
    });

    describe('@Requires decorator', () => {
        it('marks field dependencies', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;

                @Requires('email')
                profile: string;
            }

            expect(User.__fraiseqlFederation__.requires.profile).toBe('email');
        });

        it('rejects non-existent fields', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;

                @Requires('nonexistent')
                profile: string;
            }

            expect(() => {
                validateFederation([User]);
            }).toThrow('Field \'nonexistent\' not found');
        });

        it('supports multiple @Requires on different fields', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;
                @External() phone: string;

                @Requires('email')
                contact_email: string;

                @Requires('phone')
                contact_phone: string;
            }

            const requires = User.__fraiseqlFederation__.requires;
            expect(requires.contact_email).toBe('email');
            expect(requires.contact_phone).toBe('phone');
        });

        it('works on locally-owned type for reference resolution', () => {
            @Type()
            @Key('id')
            class Order {
                id: string;
                user_id: string;

                @Requires('user_id')
                user: string;
            }

            const requires = Order.__fraiseqlFederation__.requires;
            expect(requires.user).toBe('user_id');
        });
    });

    describe('@Provides decorator', () => {
        it('marks field as providing data', () => {
            @Type()
            @Key('id')
            class User {
                id: string;
                email: string;

                @Provides('Order.owner_email')
                email_field: string;
            }

            const provides = User.__fraiseqlFederation__.provides_data;
            expect(provides).toContain('Order.owner_email');
        });

        it('supports multiple targets', () => {
            @Type()
            @Key('id')
            class User {
                id: string;
                email: string;

                @Provides('Order.owner_email', 'Invoice.owner_email')
                email_reference: string;
            }

            const provides = User.__fraiseqlFederation__.provides_data;
            expect(provides).toContain('Order.owner_email');
            expect(provides).toContain('Invoice.owner_email');
        });
    });

    describe('Schema JSON Generation', () => {
        it('includes federation metadata', () => {
            @Type()
            @Key('id')
            class User {
                id: string;
                email: string;
            }

            const schema = generateSchemaJson([User]);

            expect(schema.federation).toBeDefined();
            expect(schema.federation.enabled).toBe(true);
            expect(schema.federation.version).toBe('v2');
        });

        it('includes type-level federation metadata', () => {
            @Type()
            @Key('id')
            class User {
                id: string;
                email: string;
            }

            const schema = generateSchemaJson([User]);
            const userType = schema.types.find((t: any) => t.name === 'User');

            expect(userType.federation).toBeDefined();
            expect(userType.federation.keys).toEqual([{ fields: ['id'] }]);
            expect(userType.federation.extend).toBe(false);
        });

        it('includes extended type metadata', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;
            }

            const schema = generateSchemaJson([User]);
            const userType = schema.types.find((t: any) => t.name === 'User');

            expect(userType.federation.extend).toBe(true);
            expect(userType.federation.external_fields).toContain('id');
            expect(userType.federation.external_fields).toContain('email');
        });

        it('includes field-level federation metadata', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;

                @Requires('email')
                profile: string;
            }

            const schema = generateSchemaJson([User]);
            const userType = schema.types.find((t: any) => t.name === 'User');

            const idField = userType.fields.find((f: any) => f.name === 'id');
            expect(idField.federation.external).toBe(true);

            const profileField = userType.fields.find((f: any) => f.name === 'profile');
            expect(profileField.federation.requires).toBe('email');
        });

        it('includes provides metadata', () => {
            @Type()
            @Key('id')
            class User {
                id: string;

                @Provides('Order.owner_email')
                email: string;
            }

            const schema = generateSchemaJson([User]);
            const userType = schema.types.find((t: any) => t.name === 'User');
            const emailField = userType.fields.find((f: any) => f.name === 'email');

            expect(emailField.federation.provides).toContain('Order.owner_email');
        });
    });

    describe('Compile-time Validation', () => {
        it('rejects invalid key fields', () => {
            @Key('nonexistent')
            @Type()
            class User {
                id: string;
            }

            expect(() => {
                validateFederation([User]);
            }).toThrow('Field \'nonexistent\' not found');
        });

        it('rejects @External without @Extends', () => {
            @Type()
            class User {
                @External() id: string;
            }

            expect(() => {
                validateFederation([User]);
            }).toThrow('@external requires @extends');
        });

        it('rejects @Requires with non-existent field', () => {
            @Type()
            @Key('id')
            class Order {
                id: string;

                @Requires('nonexistent')
                user: string;
            }

            expect(() => {
                validateFederation([Order]);
            }).toThrow('Field \'nonexistent\' not found');
        });

        it('rejects duplicate keys', () => {
            expect(() => {
                @Key('id')
                @Key('id')
                @Type()
                class User {
                    id: string;
                }
            }).toThrow('Duplicate key field');
        });
    });

    describe('Field Validation', () => {
        it('validates external fields exist', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                id: string;

                // @External on field that will be validated
            }

            // Note: External fields are only validated if they're actually
            // defined as @External. This test checks metadata consistency.
            const metadata = User.__fraiseqlFederation__;
            expect(metadata.extend).toBe(true);
        });

        it('allows mixed external and regular fields', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;
                orders: string[];  // Regular field
                purchases: string[];  // Another regular field
            }

            const external = User.__fraiseqlFederation__.external_fields;
            expect(external).toHaveLength(2);
            expect(external).toContain('id');
            expect(external).toContain('email');
        });

        it('validates composite keys', () => {
            @Key('tenant_id')
            @Key('id')
            @Type()
            class Account {
                tenant_id: string;
                id: string;
                name: string;
            }

            const keys = Account.__fraiseqlFederation__.keys;
            expect(keys).toHaveLength(2);
            expect(keys).toContainEqual({ fields: ['tenant_id'] });
            expect(keys).toContainEqual({ fields: ['id'] });
        });
    });

    describe('Complex Scenarios', () => {
        it('handles three-type federation', () => {
            @Type()
            @Key('id')
            class User {
                id: string;
                email: string;
            }

            @Type()
            @Key('id')
            class Order {
                id: string;
                user_id: string;

                @Requires('user_id')
                user: string;
            }

            @Type()
            @Key('id')
            class Product {
                id: string;
                name: string;
            }

            const schema = generateSchemaJson([User, Order, Product]);
            expect(schema.types).toHaveLength(3);
            expect(schema.federation.enabled).toBe(true);
        });

        it('handles mixed local and extended entities', () => {
            @Type()
            @Key('id')
            class User {
                id: string;
                email: string;
            }

            @Extends()
            @Key('id')
            @Type()
            class UserExtended {
                @External() id: string;
                @External() email: string;
                orders: string[];
            }

            const schema = generateSchemaJson([User, UserExtended]);
            const localUser = schema.types.find((t: any) => t.name === 'User');
            const extendedUser = schema.types.find((t: any) => t.name === 'UserExtended');

            expect(localUser.federation.extend).toBe(false);
            expect(extendedUser.federation.extend).toBe(true);
        });

        it('preserves metadata through schema generation', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;

                @Requires('email')
                profile: string;

                @Provides('Order.user_email')
                email_ref: string;
            }

            const schema = generateSchemaJson([User]);
            const userType = schema.types.find((t: any) => t.name === 'User');

            // Verify all metadata preserved
            expect(userType.federation.extend).toBe(true);
            expect(userType.federation.external_fields).toContain('id');
            expect(userType.federation.external_fields).toContain('email');

            const profileField = userType.fields.find((f: any) => f.name === 'profile');
            expect(profileField.federation.requires).toBe('email');

            const emailRefField = userType.fields.find((f: any) => f.name === 'email_ref');
            expect(emailRefField.federation.provides).toContain('Order.user_email');
        });
    });
});
