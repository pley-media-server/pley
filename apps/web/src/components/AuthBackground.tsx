/** The glow behind the sign-in, setup and offline screens, in the theme's accent. */
export function AuthBackground() {
  return (
    <div
      aria-hidden
      className="pointer-events-none absolute inset-0 opacity-60"
      style={{
        background:
          "radial-gradient(900px 500px at 15% -10%, color-mix(in oklab, var(--color-accent) 18%, transparent), transparent 60%), radial-gradient(700px 500px at 110% 110%, color-mix(in oklab, var(--color-info) 12%, transparent), transparent 60%)",
      }}
    />
  );
}
