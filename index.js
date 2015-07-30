import formatSpecifier from "./src/formatSpecifier";
import locale from "./src/locale";
import precisionFixed from "./src/precisionFixed";
import precisionPrefix from "./src/precisionPrefix";
import precisionRound from "./src/precisionRound";

import caEs from "./src/locale/ca-ES";
import deDe from "./src/locale/de-DE";
import enCa from "./src/locale/en-CA";
import enGb from "./src/locale/en-GB";
import enUs from "./src/locale/en-US";
import esEs from "./src/locale/es-ES";
import fiFi from "./src/locale/fi-FI";
import frCa from "./src/locale/fr-CA";
import frFr from "./src/locale/fr-FR";
import heIl from "./src/locale/he-IL";
import itIt from "./src/locale/it-IT";
import mkMk from "./src/locale/mk-MK";
import nlNl from "./src/locale/nl-NL";
import plPl from "./src/locale/pl-PL";
import ptBr from "./src/locale/pt-BR";
import ruRu from "./src/locale/ru-RU";
import zhCn from "./src/locale/zh-CN";

var localeDefinitions = (new Map)
    .set("ca-ES", caEs)
    .set("de-DE", deDe)
    .set("en-CA", enCa)
    .set("en-GB", enGb)
    .set("en-US", enUs)
    .set("es-ES", esEs)
    .set("fi-FI", fiFi)
    .set("fr-CA", frCa)
    .set("fr-FR", frFr)
    .set("he-IL", heIl)
    .set("it-IT", itIt)
    .set("mk-MK", mkMk)
    .set("nl-NL", nlNl)
    .set("pl-PL", plPl)
    .set("pt-BR", ptBr)
    .set("ru-RU", ruRu)
    .set("zh-CN", zhCn);

var defaultLocale = locale(enUs);
export var format = defaultLocale.format;
export var formatPrefix = defaultLocale.formatPrefix;

export function localeFormat(definition) {
  if (typeof definition === "string") {
    definition = localeDefinitions.get(definition);
    if (!definition) return null;
  }
  return locale(definition);
};

export {
  formatSpecifier,
  precisionFixed,
  precisionPrefix,
  precisionRound
};
