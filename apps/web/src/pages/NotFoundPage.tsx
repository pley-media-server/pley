import { Link, isRouteErrorResponse, useRouteError } from "react-router";
import { useTranslation } from "react-i18next";
import { Compass, TriangleAlert } from "lucide-react";
import { Button, buttonClass, EmptyState } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";

export function NotFoundPage() {
  const { t } = useTranslation("pages");
  useDocumentTitle(t("notFound.documentTitle"));
  return (
    <EmptyState
      className="min-h-[70vh]"
      icon={<Compass />}
      title={t("notFound.title")}
      description={t("notFound.description")}
      action={
        <Link to="/" className={buttonClass("primary")}>
          {t("notFound.goHome")}
        </Link>
      }
    />
  );
}

/** Router-level error boundary. */
export function RouteErrorPage() {
  const { t } = useTranslation(["pages", "common"]);
  const error = useRouteError();
  const message = isRouteErrorResponse(error)
    ? `${error.status} ${error.statusText}`
    : error instanceof Error
      ? error.message
      : t("notFound.unexpectedError");
  return (
    <div className="flex min-h-screen items-center justify-center">
      <EmptyState
        icon={<TriangleAlert />}
        title={t("common:states.somethingWentWrong")}
        description={message}
        action={
          <div className="flex gap-2">
            <Button onClick={() => window.location.reload()}>{t("notFound.reload")}</Button>
            <a href="/" className={buttonClass("primary")}>
              {t("notFound.goHome")}
            </a>
          </div>
        }
      />
    </div>
  );
}
