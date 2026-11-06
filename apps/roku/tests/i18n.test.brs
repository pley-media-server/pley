' The language shown and its words (components/lib/I18n.brs). The order
' the language is chosen in is the other TVs' (packages/tv/test/fixtures/
' language.json, which the runner gives as pkg:/fixtures/language.json);
' scripts/locales.mjs checks the catalogs themselves.

function i18nTestFixture() as object
    return ParseJson(ReadAsciiFile("pkg:/fixtures/language.json"))
end function

sub test_i18n_picks_the_language_as_the_other_tvs_do()
    cases = i18nTestFixture().pickLanguage
    tkTrue(cases.Count() > 5, "the fixture's cases")
    for each c in cases
        tkEqual(i18nPick(c.account, c.server, c.device, invalid), c.out, FormatJson(c))
    end for
end sub

sub test_i18n_the_one_picked_on_the_roku_comes_after_the_accounts()
    cases = i18nTestFixture().pickLanguageWithChoice
    tkTrue(cases.Count() > 3, "the fixture's cases")
    for each c in cases
        tkEqual(i18nPick(c.account, c.server, c.device, c.choice), c.out, FormatJson(c))
    end for
end sub

sub test_i18n_tags_match_by_their_language()
    tkEqual(i18nSupported("fr_CA"), "fr", "Roku's own locale")
    tkEqual(i18nSupported("fr-CA"), "fr")
    tkEqual(i18nSupported("ES_mx"), "es")
    tkEqual(i18nSupported("en_US"), "en")
    tkEqual(i18nSupported("de_DE"), "")
    tkEqual(i18nSupported(""), "")
    tkEqual(i18nSupported(invalid), "")
    tkEqual(i18nDeviceLanguage("es_MX"), "es", "one tag, as Roku gives it")
    tkEqual(i18nSystem("", "de_DE"), "en")
    tkEqual(i18nSystem("es", "de_DE"), "es")
    tkEqual(i18nSystem("es", "fr_CA"), "fr")
    tkEqual(i18nName("fr"), "Français")
    tkEqual(i18nName("es"), "Español")
end sub

sub test_i18n_counts_take_the_plural_form_cldr_gives()
    cases = [
        ["en", 0, "other"], ["en", 1, "one"], ["en", 2, "other"], ["en", 1000000, "other"],
        ["fr", 0, "one"], ["fr", 1, "one"], ["fr", 1.5, "one"], ["fr", 2, "other"], ["fr", 1000000, "many"], ["fr", 3000000, "many"], ["fr", 1000001, "other"],
        ["es", 0, "other"], ["es", 1, "one"], ["es", 1000000, "many"], ["es", 1500000, "other"]
    ]
    for each c in cases
        tkEqual(i18nPluralForm(c[0], c[1]), c[2], c[0] + " " + Str(c[1]).Trim())
    end for
end sub

sub test_i18n_numbers_are_written_the_languages_way()
    tkEqual(i18nNumber(1234), "1,234")
    tkEqual(i18nNumber(12345678), "12,345,678")
    tkEqual(i18nNumber(7.25, 1), "7.3")
    tkEqual(i18nNumber(-1500), "-1,500")
    i18nUse("fr")
    tkEqual(i18nNumber(12345), "12" + Chr(160) + "345")
    tkEqual(i18nNumber(1234), "1" + Chr(160) + "234")
    tkEqual(i18nNumber(7.5, 1), "7,5")
    i18nUse("es")
    tkEqual(i18nNumber(1234), "1234", "Spanish groups from five digits")
    tkEqual(i18nNumber(12345), "12.345")
    tkEqual(i18nNumber(0.25, 2), "0,25")
end sub

' Words of a made-up area, in place of the package's.
sub i18nTestWords(shown as string)
    i18nUse(shown)
    GetGlobalAA().i18nCache = {
        language: shown,
        areas: {
            "en/test": { a: "Hello", b: "Only in English", files_one: "{{count}} file", files_other: "{{count}} files", left_one: "one left", left_other: "{{count, number}} left", nested: { deep: "{{name}} has {{n}} of {{ total, number }}" }, missing: "{{gone}} stays" },
            "fr/test": { a: "Bonjour", files_one: "{{count}} fichier", files_many: "{{count, number}} de fichiers", files_other: "{{count}} fichiers" },
            "es/test": { a: "Hola", files_one: "{{count}} archivo", files_other: "{{count}} archivos" }
        }
    }
end sub

sub test_i18n_words_fall_back_to_english_then_to_their_key()
    i18nTestWords("fr")
    tkEqual(t("test.a"), "Bonjour")
    tkEqual(t("test.b"), "Only in English")
    tkEqual(t("test.c"), "test.c")
    tkEqual(t("nothing"), "nothing", "a key without an area")
    i18nTestWords("en")
    tkEqual(t("test.a"), "Hello")
end sub

sub test_i18n_values_fill_in_by_name()
    i18nTestWords("en")
    tkEqual(t("test.nested.deep", { name: "Ann", n: 1234, total: 5678 }), "Ann has 1234 of 5,678")
    tkEqual(t("test.missing", { name: "Ann" }), "{{gone}} stays")
    tkEqual(t("test.missing"), "{{gone}} stays")
    tkEqual(t("test.nested.deep", { name: "{{n}}", n: 1, total: 2 }), "{{n}} has 1 of 2", "a value is never read as a name")
end sub

sub test_i18n_a_count_picks_its_form()
    i18nTestWords("en")
    tkEqual(t("test.files", { count: 1 }), "1 file")
    tkEqual(t("test.files", { count: 0 }), "0 files")
    tkEqual(t("test.left", { count: 1 }), "one left")
    tkEqual(t("test.left", { count: 1234 }), "1,234 left")
    i18nTestWords("fr")
    tkEqual(t("test.files", { count: 0 }), "0 fichier")
    tkEqual(t("test.files", { count: 2 }), "2 fichiers")
    tkEqual(t("test.files", { count: 1000000 }), "1" + Chr(160) + "000" + Chr(160) + "000 de fichiers")
    tkEqual(t("test.left", { count: 3 }), "3 left", "English's when French lacks the key")
    i18nTestWords("es")
    tkEqual(t("test.files", { count: 1000000 }), "1000000 archivos", "a form the language lacks is its other")
end sub

sub test_i18n_the_package_words_are_read_in_the_language_shown()
    tkEqual(t("common.actions.cancel"), "Cancel")
    i18nUse("fr")
    tkEqual(t("common.actions.cancel"), "Annuler")
    i18nUse("es")
    tkEqual(t("common.actions.cancel"), "Cancelar")
    i18nUse("de")
    tkEqual(i18nLanguage(), "en", "a language Pley isn't in")
end sub

' What the scene knows: a Roku in French, not signed in, no server.
function i18nTestFacts(more as object) as object
    facts = { signedIn: false, accountHeard: false, account: "", server: "", serverHeard: false, serverLanguage: "", refused: false, device: "fr_CA", choice: "" }
    facts.Append(more)
    return facts
end function

sub test_i18n_a_start_shows_the_language_shown_last()
    tkEqual(i18nAtStart("es", "fr_CA"), "es")
    tkEqual(i18nAtStart("", "fr_CA"), "fr", "the first start: the Roku's")
    tkEqual(i18nAtStart("de", "de_DE"), "en", "neither known: English")
end sub

sub test_i18n_the_language_waits_for_what_picks_it()
    tkEqual(i18nShown(i18nTestFacts({})), "fr", "no server: the Roku's")
    tkEqual(i18nShown(i18nTestFacts({ signedIn: true, server: "http://nas:7539", serverHeard: true, serverLanguage: "en" })), "", "signed in: the account first")
    tkEqual(i18nShown(i18nTestFacts({ signedIn: true, accountHeard: true, account: "es", server: "http://nas:7539" })), "es", "the account's, the server unheard")
    tkEqual(i18nShown(i18nTestFacts({ signedIn: true, accountHeard: true, account: "", server: "http://nas:7539" })), "fr", "Automatic: the Roku's, which needs no server")
    tkEqual(i18nShown(i18nTestFacts({ device: "de_DE", server: "http://nas:7539" })), "", "a Roku in German waits for the server")
    tkEqual(i18nShown(i18nTestFacts({ device: "de_DE", server: "http://nas:7539", serverHeard: true, serverLanguage: "es" })), "es")
    tkEqual(i18nShown(i18nTestFacts({ device: "de_DE", server: "http://nas:7539", choice: "en" })), "en", "the one picked here needs no server")
    tkEqual(i18nShown(i18nTestFacts({ device: "de_DE", server: "http://nas:7539", serverHeard: true, serverLanguage: "" })), "en", "a server from before languages: English")
    tkEqual(i18nShown(i18nTestFacts({ signedIn: false, accountHeard: true, account: "es", choice: "en" })), "en", "signed out, the account counts no more")
end sub

sub test_i18n_the_settings_row_is_the_accounts_where_the_server_keeps_one()
    row = i18nRow(i18nTestFacts({ signedIn: true, accountHeard: true, account: "es", serverHeard: true, serverLanguage: "en", choice: "en" }))
    tkFalse(row.onDevice)
    tkEqual(row.current, "es")
    tkEqual(row.automatic, "en", "Automatic: the one picked with the globe")
    row = i18nRow(i18nTestFacts({ signedIn: true, accountHeard: true, account: "", serverLanguage: "es" }))
    tkFalse(row.onDevice, "a server not heard yet is asked")
    tkEqual(row.current, "")
    tkEqual(row.automatic, "fr", "the Roku's")
    row = i18nRow(i18nTestFacts({ signedIn: true, accountHeard: true, serverHeard: true, serverLanguage: "", choice: "es" }))
    tkTrue(row.onDevice, "a Pley from before languages names none")
    tkEqual(row.current, "es")
    tkEqual(row.automatic, "fr", "Automatic: the Roku's own, never the choice")
    row = i18nRow(i18nTestFacts({ device: "de_DE", signedIn: true, accountHeard: true, serverHeard: true, serverLanguage: "es", refused: true }))
    tkTrue(row.onDevice, "refused with a 404 or a 405")
    tkEqual(row.automatic, "es", "a Roku in German: the server's")
    row = i18nRow(i18nTestFacts({ signedIn: true, accountHeard: true, serverHeard: true, serverLanguage: "de" }))
    tkFalse(row.onDevice, "a later Pley in a language the Roku hasn't still keeps the account's")
end sub

' On a server that keeps no account's language, the one picked on the Roku
' shows at once: nothing of the account is waited for.
sub test_i18n_a_server_that_keeps_none_waits_for_no_account()
    server = "http://nas:7539"
    facts = i18nTestFacts({ signedIn: true, server: server, serverHeard: true, serverLanguage: "", choice: "es" })
    tkEqual(i18nShown(facts), "es", "a Pley from before languages")
    facts = i18nTestFacts({ signedIn: true, server: server, serverHeard: true, serverLanguage: "en", refused: true, choice: "es" })
    tkEqual(i18nShown(facts), "es", "one that answered a choice 404")
    facts = i18nTestFacts({ signedIn: true, server: server, serverHeard: true, serverLanguage: "de", choice: "es" })
    tkEqual(i18nShown(facts), "", "one that keeps an account's: its language first")
end sub

sub test_i18n_the_choices_name_automatic_and_each_language_in_itself()
    choice = i18nChoices("fr", "")
    tkEqual(choice.labels, ["Automatic (Français)", "English", "Français", "Español"])
    tkEqual(choice.values, ["", "en", "fr", "es"])
    tkEqual(choice.current, 0)
    i18nUse("es")
    choice = i18nChoices("en", "es")
    tkEqual(choice.labels[0], "Automático (English)")
    tkEqual(choice.current, 3)
end sub

sub test_i18n_errors_and_sign_in_speak_the_language_shown()
    tkEqual(httpStatusSentence(500), "The server couldn't answer (HTTP 500).")
    i18nUse("fr")
    tkEqual(httpStatusSentence(500), "Le serveur n’a pas pu répondre (HTTP 500).")
    tkEqual(httpAnswer(0, "").message, "Le serveur est injoignable.")
    tkEqual(profNeedsLabel("password"), "Mot de passe")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""wrong_pin""}").message, "Code PIN incorrect.")
    i18nUse("es")
    tkEqual(authDeviceRefusal(429, ""), "Hay demasiados inicios de sesión esperando en este servidor: vuelve a intentarlo en unos minutos.")
    tkEqual(profStartAsLabel([], 3), "Último perfil usado")
end sub
