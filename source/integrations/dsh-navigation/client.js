window.__ModuleLoader__.load({
  id: "agent-hub-dsh-navigation",
  factory: () => ({
    inject: ["uiWorkspace", "sessions"],
    apply(ctx) {
      // Web browser tabs must never steal a desktop navigation request.
      if (window.location.protocol !== "dsh-app:") return;
      let stopped = false,
        timer;
      const current = () =>
        Object.values(ctx.sessions.list.getSnapshot().byId).find(
          (row) => (row.retainedBy.mainView ?? 0) > 0,
        )?.id ?? null;
      const send = async (extra = {}) => {
        const response = await fetch("/agent-hub-navigation/client", {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "x-agent-hub-client": "1",
          },
          body: JSON.stringify({
            desktop: true,
            selected: current(),
            ...extra,
          }),
          signal: AbortSignal.timeout(2000),
        });
        if (!response.ok) throw new Error("Navigation bridge unavailable");
        return response.json();
      };
      const tick = async () => {
        try {
          const { pending } = await send();
          if (
            !stopped &&
            pending &&
            /^[A-Za-z0-9_-]{1,128}$/.test(pending.sessionId)
          ) {
            let ok = false;
            try {
              ctx.uiWorkspace.openSession(pending.sessionId);
              const deadline = Date.now() + 4000;
              while (!stopped && Date.now() < deadline) {
                const binding = ctx.sessions.binding(pending.sessionId);
                if (
                  current() === pending.sessionId &&
                  binding?.session.getSnapshot().openState === "open"
                ) {
                  ok = true;
                  break;
                }
                await new Promise((resolve) => setTimeout(resolve, 50));
              }
            } finally {
              if (!stopped) await send({ requestId: pending.requestId, ok });
            }
          }
        } catch {
          /* no retries of a claimed request; native caller sees failure */
        }
        if (!stopped) timer = setTimeout(tick, 500);
      };
      ctx.effect(() => {
        void tick();
        return () => {
          stopped = true;
          clearTimeout(timer);
        };
      });
    },
  }),
});
