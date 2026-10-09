# snapr-digits-bold.ttf

Only the digits 0–9 of Carlito Bold (© 2010-2013 tyPoland Lukasz Dziedzic), renamed as required
for a modified version. Licensed under the SIL Open Font License 1.1, see `OFL.txt`.

Used for the numbered markers only when Segoe UI Bold (the font of the C# version) is not
installed. Regenerate with fontTools:

    pyftsubset Carlito-Bold.ttf --text=0123456789 --output-file=snapr-digits-bold.ttf
    (then replace "Carlito" in the name table)
