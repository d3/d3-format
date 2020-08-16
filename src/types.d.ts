declare module 'd3-format' {
    function formatDefaultLocale(definition: LocaleDefinition): LocaleFormatter
    function format(specifier: string): (value: number) => string
    function formatPrefix(specifier: string, value: number): string

    function formatLocale(definition: LocaleDefinition): LocaleFormatter
    
    function formatSpecifier(specifier: string): FormatSpecifier

    function precisionFixed(step: number): number
    function precisionPrefix(step: number, value: number): number
    function precisionRound(step: number, max: number): number

    class FormatSpecifier {
        constructor(specifier: FormatSpecifier)

        fill?: string
        align?: string
        sign?: string
        symbol?: string
        zero?: boolean
        width?: number
        comma?: boolean
        precision?: number
        trim?: boolean
        type?: string
    }

    interface LocaleDefinition {
        decimal?: string
        thousands?: string
        grouping?: number[]
        currency?: string[]
        numerals?: string[]
        percent?: string
        minus?: string
        nan?: string
    }

    interface LocaleFormatter {
        format(specifier: string): (value: number) => string
        formatPrefix(specifier: string, value: number): string
    }
}