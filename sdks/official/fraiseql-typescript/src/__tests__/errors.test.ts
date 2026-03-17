import {
  FraiseQLError,
  GraphQLError,
  NetworkError,
  TimeoutError,
  AuthenticationError,
  RateLimitError,
} from '../errors';

describe('Error hierarchy', () => {
  it('FraiseQLError is an Error', () => {
    const err = new FraiseQLError('base error');
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(FraiseQLError);
    expect(err.name).toBe('FraiseQLError');
    expect(err.message).toBe('base error');
  });

  it('GraphQLError carries errors array', () => {
    const entries = [
      { message: 'field not found', path: ['user', 'name'] as Array<string | number> },
    ];
    const err = new GraphQLError(entries);
    expect(err).toBeInstanceOf(FraiseQLError);
    expect(err.name).toBe('GraphQLError');
    expect(err.message).toBe('field not found');
    expect(err.errors).toEqual(entries);
  });

  it('GraphQLError with empty array uses fallback message', () => {
    const err = new GraphQLError([]);
    expect(err.message).toBe('GraphQL error');
  });

  it('NetworkError is a FraiseQLError', () => {
    const err = new NetworkError('connection refused');
    expect(err).toBeInstanceOf(FraiseQLError);
    expect(err.name).toBe('NetworkError');
  });

  it('TimeoutError is a NetworkError', () => {
    const err = new TimeoutError();
    expect(err).toBeInstanceOf(NetworkError);
    expect(err).toBeInstanceOf(FraiseQLError);
    expect(err.name).toBe('TimeoutError');
    expect(err.message).toBe('Request timed out');
  });

  it('AuthenticationError carries statusCode', () => {
    const err401 = new AuthenticationError(401);
    expect(err401).toBeInstanceOf(FraiseQLError);
    expect(err401.name).toBe('AuthenticationError');
    expect(err401.statusCode).toBe(401);
    expect(err401.message).toContain('401');

    const err403 = new AuthenticationError(403);
    expect(err403.statusCode).toBe(403);
  });

  it('RateLimitError carries optional retryAfterMs', () => {
    const errNoMs = new RateLimitError();
    expect(errNoMs).toBeInstanceOf(FraiseQLError);
    expect(errNoMs.name).toBe('RateLimitError');
    expect(errNoMs.retryAfterMs).toBeUndefined();

    const errWithMs = new RateLimitError(5000);
    expect(errWithMs.retryAfterMs).toBe(5000);
  });
});
