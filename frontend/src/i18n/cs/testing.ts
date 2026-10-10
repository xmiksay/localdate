// Alpha / beta testing tools: test users, impersonation ("act as"), the audit log.
export default {
  impersonation: {
    actAs: 'Přihlásit se jako',
    actAsLabel: 'Přihlásit se jako @{username}',
    banner: 'Jednáte jako {username}',
    stop: 'Ukončit',
    expired: 'Přihlášení za uživatele vypršelo — jste zpět jako admin',
    banned: 'Uživatel byl zablokován — jste zpět jako admin',
    dismiss: 'Rozumím',
    settingsNote:
      'Při jednání za jiného uživatele nelze měnit heslo, propojené účty, oznámení ani blokování a nelze smazat účet.',
  },
  position: {
    title: 'Poloha',
    intro:
      'Poloha se teď nesdílí sama. Nastav ji polohou tohoto zařízení nebo klepnutím či přetažením značky v mapě.',
    useDevice: 'Použít polohu tohoto zařízení',
    duration: 'Délka nové viditelnosti',
    startsWindow: 'Viditelnost není zapnutá: nastavením polohy se v tom bodě zapne.',
    current: 'Nastaveno: {lat}, {lon}',
    none: 'Poloha zatím není nastavená.',
    saving: 'Ukládám polohu…',
    mapLabel: 'Mapa pro výběr polohy',
  },
  adminUsers: {
    searchLabel: 'Hledat uživatele',
    searchHint: 'Uživatelské nebo zobrazované jméno; prázdné pole ukáže nejnovější účty.',
    empty: 'Nikdo nenalezen.',
    age: '{n} let',
  },
  testUsers: {
    intro:
      'Testovací účty nemají heslo, ovládáš je přes „Přihlásit se jako“. Ostatní uživatelé je vidí jako kohokoli jiného.',
    add: 'Nový testovací uživatel',
    empty: 'Zatím žádní testovací uživatelé.',
    username: 'Uživatelské jméno',
    usernameInvalid: '1 až 64 znaků, aspoň jeden viditelný, bez řídicích znaků.',
    gender: 'Pohlaví',
    photo: 'Fotka (nepovinné)',
    photoHint: 'Bez fotky se vygeneruje zástupný avatar.',
    create: 'Vytvořit',
    delete: 'Smazat',
    deleteTitle: 'Smazat @{username}?',
    deleteBody:
      'Účet se nenávratně smaže i se vším, co k němu patří, stejně jako při smazání účtu.',
    photoFailed: 'Účet @{username} je vytvořený, ale fotku se nepodařilo nahrát: {error}',
    impersonationOff: 'Přihlášení za uživatele je na serveru vypnuté (ADMIN_IMPERSONATION).',
  },
  audit: {
    intro: 'Posledních 200 záznamů o přihlášení za uživatele a o změnách, které při něm proběhly.',
    empty: 'Zatím žádné záznamy.',
    action: {
      impersonate: 'Přihlášení za uživatele',
      impersonated_request: 'Požadavek za uživatele',
    },
    deleted: 'smazaný účet',
  },
}
