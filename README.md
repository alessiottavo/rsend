# Usage

rsend requires no configuration, no accounts, and no servers. Two commands, one for each side.

---

## Sending

```
rsend send <file|directory>
```

```
$ rsend send ./secrets/

your alias:   amber-wolf
pairing code: yellow_button_freak

waiting to pair...

paired with: orange-falcon!
waiting for consent...

sending [████████░░░░░░░░░░░░] 40% (1.2 MB/s)
sending [████████████████████] 100%

done!
```

---

## Receiving

```
rsend get [directory]
```

If no directory is provided, files are received into the current working directory.

```
$ rsend get ./incoming/

receiving to: /home/user/incoming/
pairing code: yellow_button_freak

paired with: amber-wolf!

incoming transfer:

  secrets/
  ├── .env.production     (2.4 KB)
  ├── .env.staging        (1.8 KB)
  └── api_keys.json       (4.1 KB)

  total: 3 files, 8.3 KB

  ⚠ .env.production will overwrite an existing file

accept? [y/n] y

receiving [████████████████████] 100%

done!
```

---

## Alias Verification

After pairing, both sides display each other's alias.

Before accepting the transfer, verify the alias matches your peer's terminal output over a voice call or in person. A mismatch indicates a man-in-the-middle attack — abort immediately.

```
sender sees:   paired with: orange-falcon!
receiver sees: paired with: amber-wolf!
```

---
