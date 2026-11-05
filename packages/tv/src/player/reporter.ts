/**
 * Sends playbacks' reports one after the other: a player that closes still
 * gets its last position and its session's end to the server, in the
 * order they were made (progress, then `stopped`, then DELETE). Best
 * effort: a failure is dropped, as on the web and Android.
 */
export class Reporter {
  private tail: Promise<void> = Promise.resolve();
  /** Counts `drop`s: a report queued before the last one doesn't go out. */
  private generation = 0;

  send(report: () => Promise<unknown>): void {
    const generation = this.generation;
    this.tail = this.tail.then(
      () =>
        generation !== this.generation
          ? undefined
          : report().then(
              () => undefined,
              () => undefined,
            ),
      () => undefined,
    );
  }

  /** Resolves once everything queued so far went out, or after `timeoutMs`. */
  flush(timeoutMs: number): Promise<void> {
    return new Promise((resolve) => {
      const timer = setTimeout(resolve, timeoutMs);
      this.tail.then(() => {
        clearTimeout(timer);
        resolve();
      });
    });
  }

  /**
   * What is queued and hasn't gone out yet never does: the sign-in it was
   * made with is about to change (a cast's profile switch), and it would go
   * out with the new one. One on its way already ends as it may.
   */
  drop(): void {
    this.generation++;
  }
}
