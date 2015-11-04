import locale from "./src/locale";
import caEs from "./src/locale/ca-ES";
import deCh from "./src/locale/de-CH";
import deDe from "./src/locale/de-DE";
import enCa from "./src/locale/en-CA";
import enGb from "./src/locale/en-GB";
import enUs from "./src/locale/en-US";
import esEs from "./src/locale/es-ES";
import fiFi from "./src/locale/fi-FI";
import frCa from "./src/locale/fr-CA";
import frFr from "./src/locale/fr-FR";
import heIl from "./src/locale/he-IL";
import huHu from "./src/locale/hu-HU";
import itIt from "./src/locale/it-IT";
import jaJp from "./src/locale/ja-JP";
import koKr from "./src/locale/ko-KR";
import mkMk from "./src/locale/mk-MK";
import nlNl from "./src/locale/nl-NL";
import plPl from "./src/locale/pl-PL";
import ptBr from "./src/locale/pt-BR";
import ruRu from "./src/locale/ru-RU";
import svSe from "./src/locale/sv-SE";
import zhCn from "./src/locale/zh-CN";

var localeDefinitions = {
  "ca-ES": caEs,
  "de-CH": deCh,
  "de-DE": deDe,
  "en-CA": enCa,
  "en-GB": enGb,
  "en-US": enUs,
  "es-ES": esEs,
  "fi-FI": fiFi,
  "fr-CA": frCa,
  "fr-FR": frFr,
  "he-IL": heIl,
  "hu-HU": huHu,
  "it-IT": itIt,
  "ja-JP": jaJp,
  "ko-KR": koKr,
  "mk-MK": mkMk,
  "nl-NL": nlNl,
  "pl-PL": plPl,
  "pt-BR": ptBr,
  "ru-RU": ruRu,
  "sv-SE": svSe,
  "zh-CN": zhCn
};

var defaultLocale = locale(enUs);
export var format = defaultLocale.format;
export var formatPrefix = defaultLocale.formatPrefix;

export function localeFormat(definition) {
  if (typeof definition === "string") {
    if (!localeDefinitions.hasOwnProperty(definition)) return null;
    definition = localeDefinitions[definition];
  }
  return locale(definition);
};

export {default as formatSpecifier} from "./src/formatSpecifier";
export {default as precisionFixed} from "./src/precisionFixed";
export {default as precisionPrefix} from "./src/precisionPrefix";
export {default as precisionRound} from "./src/precisionRound";
