import { executeWithRetry } from '../http-retry';
import { NetworkError, TimeoutError, FraiseQLError, GraphQLError } from '../errors';

jest.useFakeTimers();

describe('executeWithRetry', () => {
  beforeEach(() => {
    jest.clearAllTimers();
  });

  it('returns immediately on success', async () => {
    const fn = jest.fn().mockResolvedValue('ok');
    const result = await executeWithRetry(fn, { maxAttempts: 3 });
    expect(result).toBe('ok');
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('does not retry by default (maxAttempts=1)', async () => {
    const fn = jest.fn().mockRejectedValue(new NetworkError('down'));
    await expect(executeWithRetry(fn)).rejects.toBeInstanceOf(NetworkError);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('retries NetworkError up to maxAttempts', async () => {
    const fn = jest
      .fn()
      .mockRejectedValueOnce(new NetworkError('fail 1'))
      .mockRejectedValueOnce(new NetworkError('fail 2'))
      .mockResolvedValue('success');

    const promise = executeWithRetry(fn, {
      maxAttempts: 3,
      baseDelayMs: 100,
      jitter: false,
    });

    await jest.runAllTimersAsync();
    const result = await promise;
    expect(result).toBe('success');
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it('retries TimeoutError', async () => {
    const fn = jest
      .fn()
      .mockRejectedValueOnce(new TimeoutError())
      .mockResolvedValue('data');

    const promise = executeWithRetry(fn, {
      maxAttempts: 2,
      baseDelayMs: 50,
      jitter: false,
    });

    await jest.runAllTimersAsync();
    const result = await promise;
    expect(result).toBe('data');
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('does NOT retry non-retryable errors by default', async () => {
    const fn = jest.fn().mockRejectedValue(new GraphQLError([{ message: 'bad query' }]));
    await expect(
      executeWithRetry(fn, { maxAttempts: 3 })
    ).rejects.toBeInstanceOf(GraphQLError);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('calls onRetry callback with attempt and error', async () => {
    const onRetry = jest.fn();
    const fn = jest
      .fn()
      .mockRejectedValueOnce(new NetworkError('err'))
      .mockResolvedValue('ok');

    const promise = executeWithRetry(fn, {
      maxAttempts: 2,
      baseDelayMs: 10,
      jitter: false,
      onRetry,
    });

    await jest.runAllTimersAsync();
    await promise;

    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onRetry).toHaveBeenCalledWith(1, expect.any(NetworkError));
  });

  it('throws last error after all attempts exhausted', async () => {
    jest.useRealTimers();
    const fn = jest.fn().mockImplementation(() =>
      Promise.reject(new NetworkError('always fails'))
    );

    await expect(
      executeWithRetry(fn, {
        maxAttempts: 3,
        baseDelayMs: 1,
        jitter: false,
      })
    ).rejects.toBeInstanceOf(NetworkError);
    expect(fn).toHaveBeenCalledTimes(3);
    jest.useFakeTimers();
  });

  it('respects custom retryOn list', async () => {
    class CustomError extends FraiseQLError {}

    const fn = jest
      .fn()
      .mockRejectedValueOnce(new CustomError('custom'))
      .mockResolvedValue('recovered');

    const promise = executeWithRetry(fn, {
      maxAttempts: 2,
      baseDelayMs: 10,
      jitter: false,
      retryOn: [CustomError as new (...args: never[]) => FraiseQLError],
    });

    await jest.runAllTimersAsync();
    const result = await promise;
    expect(result).toBe('recovered');
  });
});
