# Sosial Video

Sosyal video paylaşımı için bir uygulama deposu. Geliştirme [nordlyse/sosial-video](https://github.com/nordlyse/sosial-video) üzerinde yürür.

## Durum

Proje henüz iskelet aşamasındadır. Kaynak kod, çalışma talimatları ve mimari ayrıntılar sonraki commit’lerde eklenecektir.

Çalışma dalı: **develop**. `main` kararlı / yayın hattıdır.

## Amac

Bir kisi eger video baslattiginda, arkadaslari bundan haberdar olmali ve eger isterlerse video ile baglanti kurabilirler ve yayinci bunu kabul ettiginde konusmaci yada sadece dinleyici olarak kabul edebilir. Eger konusmaci olarak kabul ederse, el kaldirip video da goruslerini vb. bildirebilir boylece tartisma yapilabilir, eger sadece dinleyici olarak kabul edilirse gorus bildiremez ama dinleme yapabilir, videosunu acabilir.  Ama hem dinleyiciler hemde konusma yapabilen katilimcilar canli olarak ifadelerini ve yorumlarini yapabilirler. ornegin, begenme ifades, begenmeme iconu, kizgin surat, vb. ifadeleri canli olarak birakabilirler ve bunlar videonun altinda sayi olarak gozur yani istatistik olarak 10 kisi begendi, 5 kisi begenmedi vb. Ayrica yorumlar da videonun altinda bulunabilir.   
  
Eger arkadasi degilse bile public olarak yayin yaparsa public yayin yapanlarin listesi ayri bir listede tum baglanan kisilere gozuksun, isterler ise baglanabilir ama sadece dinleyici olarak. Eger private olarak actiysa sadece arkadaslari gorebilsin public listede gozukmesin. Canli yayinda tum public yayinlar herkese gorulebilir, ama private yayinlar sadece arkdas guruplari tarafindan gorulebilmeli.

  
Videolar, yorumlar vb. 1 yil boyunca saklanacak ama eger yayini acan kisi isterse bu videoyi silebilir ama yinede 1 sene boyunca server da kalmali.

## Serviceler

Serviceler docker ile build edilmeli ve docker compose icinde kullanilabilmeli.

## Yazar


|         |                                                     |
| ------- | --------------------------------------------------- |
| Ad      | Jakob Lyse                                          |
| GitHub  | [nordlyse](https://github.com/nordlyse)             |
| E-posta | [jakob.lyse@gmail.com](mailto:jakob.lyse@gmail.com) |


