import {terser} from "rollup-plugin-terser";
import * as meta from "./package.json";

const copyright = `// ${meta.homepage} v${meta.version} Copyright ${(new Date).getFullYear()} ${meta.author.name}`;

export default [
  {
    input: "src/index.js",
    external: Object.keys(meta.dependencies || {}).filter(key => /^d3-/.test(key)),
    output: {
      file: `dist/${meta.name}.min.js`,
      name: "d3",
      format: "umd",
      indent: false,
      extend: true,
      banner: copyright,
      globals: Object.assign({}, ...Object.keys(meta.dependencies || {}).filter(key => /^d3-/.test(key)).map(key => ({[key]: "d3"})))
    },
    plugins: [
      terser({
        output: {
          preamble: copyright
        }
      })
    ]
  }
];
