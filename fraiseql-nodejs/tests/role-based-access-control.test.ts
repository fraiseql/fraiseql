import { describe, it, expect } from '@jest/globals';
import {
  RoleRequiredBuilder,
  RoleMatchStrategy,
  RoleRequired,
} from '../src/security';

describe('Role-Based Access Control', () => {
  it('should create single role requirement', () => {
    const config = new RoleRequiredBuilder()
      .roles('admin')
      .description('Admin role required')
      .build();

    expect(config.roles).toHaveLength(1);
    expect(config.roles).toContain('admin');
  });

  it('should create multiple role requirements', () => {
    const config = new RoleRequiredBuilder()
      .roles('manager', 'director')
      .description('Manager or director required')
      .build();

    expect(config.roles).toHaveLength(2);
    expect(config.roles).toContain('manager');
    expect(config.roles).toContain('director');
  });

  it('should create roles from array', () => {
    const config = new RoleRequiredBuilder()
      .rolesArray(['viewer', 'editor', 'admin'])
      .description('Multiple roles via array')
      .build();

    expect(config.roles).toHaveLength(3);
  });

  it('should support ANY matching strategy', () => {
    const config = new RoleRequiredBuilder()
      .roles('manager', 'director')
      .strategy(RoleMatchStrategy.ANY)
      .description('User needs at least one role')
      .build();

    expect(config.strategy).toBe(RoleMatchStrategy.ANY);
  });

  it('should support ALL matching strategy', () => {
    const config = new RoleRequiredBuilder()
      .roles('admin', 'auditor')
      .strategy(RoleMatchStrategy.ALL)
      .description('User needs all roles')
      .build();

    expect(config.strategy).toBe(RoleMatchStrategy.ALL);
  });

  it('should support EXACTLY matching strategy', () => {
    const config = new RoleRequiredBuilder()
      .roles('admin')
      .strategy(RoleMatchStrategy.EXACTLY)
      .description('User must have exactly these roles')
      .build();

    expect(config.strategy).toBe(RoleMatchStrategy.EXACTLY);
  });

  it('should support role hierarchy', () => {
    const config = new RoleRequiredBuilder()
      .roles('user')
      .hierarchy(true)
      .description('Role hierarchy enabled')
      .build();

    expect(config.hierarchy).toBe(true);
  });

  it('should support role inheritance', () => {
    const config = new RoleRequiredBuilder()
      .roles('editor')
      .inherit(true)
      .description('Inherit role requirements')
      .build();

    expect(config.inherit).toBe(true);
  });

  it('should support operation-specific rules', () => {
    const config = new RoleRequiredBuilder()
      .roles('admin')
      .operations('delete,create')
      .description('Admin for destructive operations')
      .build();

    expect(config.operations).toBe('delete,create');
  });

  it('should support caching', () => {
    const config = new RoleRequiredBuilder()
      .roles('viewer')
      .cacheable(true)
      .cacheDurationSeconds(1800)
      .build();

    expect(config.cacheable).toBe(true);
    expect(config.cacheDurationSeconds).toBe(1800);
  });

  it('should support custom error message', () => {
    const config = new RoleRequiredBuilder()
      .roles('admin')
      .errorMessage('You must be an administrator to access this resource')
      .build();

    expect(config.errorMessage).toBe(
      'You must be an administrator to access this resource'
    );
  });

  it('should support fluent chaining', () => {
    const config = new RoleRequiredBuilder()
      .roles('manager', 'director')
      .strategy(RoleMatchStrategy.ANY)
      .hierarchy(true)
      .description('Manager or director with hierarchy')
      .errorMessage('Insufficient role')
      .operations('read,update')
      .inherit(false)
      .cacheable(true)
      .cacheDurationSeconds(900)
      .build();

    expect(config.roles).toHaveLength(2);
    expect(config.strategy).toBe(RoleMatchStrategy.ANY);
    expect(config.hierarchy).toBe(true);
    expect(config.inherit).toBe(false);
    expect(config.cacheDurationSeconds).toBe(900);
  });

  it('should support admin pattern', () => {
    const config = new RoleRequiredBuilder()
      .roles('admin')
      .strategy(RoleMatchStrategy.EXACTLY)
      .hierarchy(true)
      .description('Full admin access with hierarchy')
      .build();

    expect(config.roles).toHaveLength(1);
    expect(config.hierarchy).toBe(true);
  });

  it('should support manager pattern', () => {
    const config = new RoleRequiredBuilder()
      .roles('manager', 'director', 'executive')
      .strategy(RoleMatchStrategy.ANY)
      .description('Management tier access')
      .operations('read,create,update')
      .build();

    expect(config.roles).toHaveLength(3);
    expect(config.operations).toBe('read,create,update');
  });

  it('should support data scientist pattern', () => {
    const config = new RoleRequiredBuilder()
      .roles('data_scientist', 'analyst')
      .strategy(RoleMatchStrategy.ANY)
      .description('Data access for scientists and analysts')
      .operations('read')
      .build();

    expect(config.roles).toHaveLength(2);
  });

  it('should support decorator syntax', () => {
    @RoleRequired({
      roles: ['admin'],
      description: 'Admin access required',
    })
    class AdminPanel {
      content: string;
    }

    expect(AdminPanel).toBeDefined();
  });

  it('should support decorator with strategy', () => {
    @RoleRequired({
      roles: ['manager', 'director'],
      strategy: RoleMatchStrategy.ANY,
      description: 'Management access',
    })
    class SalaryData {
      employeeId: string;
      salary: number;
    }

    expect(SalaryData).toBeDefined();
  });

  it('should support decorator with all parameters', () => {
    @RoleRequired({
      roles: ['admin', 'auditor'],
      strategy: RoleMatchStrategy.ALL,
      hierarchy: true,
      description: 'Full admin with auditor',
      errorMessage: 'Insufficient privileges',
      operations: 'delete,create',
      inherit: false,
      cacheable: true,
      cacheDurationSeconds: 1200,
    })
    class ComplexRoleRequirement {
      data: string;
    }

    expect(ComplexRoleRequirement).toBeDefined();
  });

  it('should create multiple roles with different strategies', () => {
    const any = new RoleRequiredBuilder()
      .roles('editor', 'contributor')
      .strategy(RoleMatchStrategy.ANY)
      .build();

    const all = new RoleRequiredBuilder()
      .roles('editor', 'reviewer')
      .strategy(RoleMatchStrategy.ALL)
      .build();

    const exactly = new RoleRequiredBuilder()
      .roles('admin')
      .strategy(RoleMatchStrategy.EXACTLY)
      .build();

    expect(any.strategy).toBe(RoleMatchStrategy.ANY);
    expect(all.strategy).toBe(RoleMatchStrategy.ALL);
    expect(exactly.strategy).toBe(RoleMatchStrategy.EXACTLY);
  });
});
