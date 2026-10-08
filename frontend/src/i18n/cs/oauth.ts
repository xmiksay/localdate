// Sign in with an OAuth / OpenID Connect provider (#15). `{provider}` is an `oauth.provider` name;
// the callback errors carry no provider, so their texts stay provider-neutral.
export default {
  oauth: {
    provider: { google: 'Google' },
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
      unauthorized: 'Tvoje přihlášení vypršelo. Přihlas se a zkus to znovu.',
      rate_limited: 'Příliš mnoho pokusů. Chvíli počkej a zkus to znovu.',
      internal: 'Něco se pokazilo na naší straně. Zkus to prosím později.',
      unknown: 'Přihlášení se nepovedlo.',
    },
  },
}
