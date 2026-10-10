// Sign in with an OAuth / OpenID Connect provider (#15). `{provider}` is an `oauth.provider` name;
// the callback errors carry no provider, so their texts stay provider-neutral.
export default {
  oauth: {
    provider: { google: 'Google', telegram: 'Telegram', facebook: 'Facebook' },
    // "{from}" is `from.<provider>`: Czech needs the genitive ("z Facebooku").
    from: { google: 'z Googlu', telegram: 'z Telegramu', facebook: 'z Facebooku' },
    importPhoto: 'Importovat profilovou fotku {from}',
    importPhotoNew: 'U nového účtu importovat profilovou fotku {from}',
    // The done page does not know the provider of a login code, so these stay provider-neutral.
    photo: {
      imported: 'Profilová fotka je přidaná mezi tvoje fotky.',
      pending: 'Profilovou fotku přidáme k účtu, jakmile si vybereš jméno.',
      full: 'Profilovou fotku jsme nepřidali: máš už plný počet fotek.',
      none: 'Účet nemá profilovou fotku, nebylo co importovat.',
      failed: 'Profilovou fotku se nepodařilo importovat. Můžeš ji nahrát ručně.',
    },
    continue: 'Pokračovat přes {provider}',
    link: 'Propojit {provider}',
    linked: 'Účet {provider} je propojený.',
    signupTitle: 'Dokonči registraci',
    signupIntro: 'Účet je ověřený. Vyber si uživatelské jméno.',
    invalid: 'Přihlášení vypršelo nebo už bylo použito. Zkus to znovu.',
    error: {
      cancelled: 'Přihlášení bylo zrušeno.',
      invalid_state:
        'Přihlášení vypršelo nebo bylo zahájeno v jiném prohlížeči. Zkus to prosím znovu.',
      oauth_failed: 'Poskytovatel přihlášení ho nepotvrdil. Zkus to znovu.',
      provider_disabled: 'Tento způsob přihlášení teď není dostupný.',
      banned: 'Tento účet byl zablokován.',
      identity_taken: 'Tento účet už je propojený s jiným uživatelem.',
      identity_mismatch:
        'K tvému účtu je propojený jiný účet tohoto poskytovatele. Přihlas se u poskytovatele tím propojeným a zkus to znovu.',
      unauthorized: 'Tvoje přihlášení vypršelo. Přihlas se a zkus to znovu.',
      rate_limited: 'Příliš mnoho pokusů. Chvíli počkej a zkus to znovu.',
      internal: 'Něco se pokazilo na naší straně. Zkus to prosím později.',
      unknown: 'Přihlášení se nepovedlo.',
    },
  },
}
