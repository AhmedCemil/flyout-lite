# FlyoutLite

**Hafif bir Windows 11 medya flyout uygulaması.** Oyununuzu dondurmadan, her yerden oynat/duraklat/sonraki tuşlarına basın.

[🇬🇧 English README](README.md)

---

## Nedir bu?

FlyoutLite, medya tuşlarına (Oynat/Duraklat, Sonraki, Önceki) bastığınızda Fluent tasarımında bir medya kartı gösteren küçük, yerel bir Windows 11 aracıdır. Çalan parçanın adını, sanatçısını ve albüm kapağını; oynatma düğmeleriyle ve tıklayarak konum değiştirilebilen bir seek çubuğuyla birlikte gösterir. Sistem tepsisinden çalışır ve yolunuza çıkmaz.

**Rust** ve **Direct2D + DirectComposition** ile doğrudan Win32 üzerine kuruludur — .NET çalışma zamanı yok, XAML yok, Electron yok, arka plan servisi yok. Yayın çıktısı yaklaşık **250 KB**.

## Neden bir tane daha?

Benzer işi yapan iki harika proje zaten var:

- [**ModernFlyouts**](https://github.com/ModernFlyouts-Community/ModernFlyouts) (C#/WPF) — eski Windows flyout'larının cilalı, özellik dolu bir alternatifi.
- [**FluentFlyout**](https://github.com/unchihugo/FluentFlyout) (C#/WinUI) — Fluent stilinde modern, temiz bir yeniden yorumlama.

Her ikisi de bu projeye ilham verdi ve eğer sizin sisteminizde sorunsuz çalışıyorlarsa **ikisini de öneririm**.

FlyoutLite şu nedenle başladı: kendi makinemde mevcut seçenekler, tam ekran oyunlarda (özellikle Rocket League) her medya tuşuna bastığımda **birkaç saniyelik girdi gecikmesine** sebep oluyordu — bir gol kaçırmaya yetecek kadar uzun. XAML/.NET yığını her tuş basışında soğuk yoldan UI çalışması yapıyor ve bu, exclusive-fullscreen sunumla iyi geçinmiyor.

Yani bu proje, aynı fikrin farklı bir hedefle yapılmış odaklı bir yeniden yazımıdır: **ön plandaki uygulamayı asla takılmaya zorlamayan, sıfıra yakın ek yüklü bir flyout.** Flyout penceresi başlangıçta önceden oluşturulur, çizim yolu kalıcı bir Direct2D cihazı ve composition swapchain kullanır; medya tuşları ise olayları asla tüketmeyen bir `WH_KEYBOARD_LL` hook'u ile yakalanır. Sıcak yolda hiçbir bellek tahsisi yapılmaz.

Eğer sizde bu gecikme sorunu yoksa, **ModernFlyouts ve FluentFlyout harikadır** ve büyük olasılıkla size daha iyi uyacaktır — çok daha fazla özellikleri var. FlyoutLite kasıtlı olarak minimaldir.

### Paylaşılan kod yok

FlyoutLite, **FluentFlyout veya ModernFlyouts'tan hiçbir kod içermez.** Farklı dil, farklı UI yığını, farklı mimari — yalnızca üst düzey fikir ortaktır. Bu iki proje yukarıda yalnızca iyi bir modern flyout'un nasıl göründüğünü gösterdikleri ve bu tür bir aracın var olmasında payları olduğu için kredilendirilmiştir.

## Özellikler

- Albüm kapağı, parça adı, sanatçı
- Önceki / Oynat-Duraklat / Sonraki düğmeleri
- Tıklayarak konum atlatılabilen seek çubuğu
- Mica arka plan, yuvarlatılmış köşeler, vurgu rengi, Windows açık/koyu temasını takip eder
- Sağ tıklayınca menü açılan tepsi simgesi (Ayarlar, Başlangıçta çalıştır, Çıkış)
- Özel çizilmiş Ayarlar penceresi: 9 sabitleme konumu, özel X/Y, kenar boşluğu ayarı, görünür süre ayarı, kompakt mod ve başlangıçta çalıştırma anahtarı
- **Kompakt mod** — yalnızca kapak + başlık + sanatçı içeren, kontrolsüz 280×64'lük mini kart
- Medya tuşlarına müdahale etmez (her zaman `CallNextHookEx` çağrılır)
- Exclusive-fullscreen uygulamalarında otomatik gizlenir
- Başlangıçta çalıştır (HKCU `Run` anahtarı; servis yok, zamanlanmış görev yok)

## Kurulum

[Releases](https://github.com/AhmedCemil/flyout-lite/releases) sayfasından `flyout-lite.exe` dosyasını indirin ve çalıştırın. Kurulum sihirbazı yok — exe tek başına çalışır.

Windows ile birlikte otomatik açılması için tepsi simgesine sağ tıklayın → **Başlangıçta çalıştır**.

## Kaynaktan derleme

Gerekenler:
- Rust (stable, MSVC toolchain) — [rustup](https://rustup.rs) üzerinden kurulur
- Windows 11 SDK + MSVC derleme araçları (Visual Studio Build Tools 2022 veya 2026, "Desktop development with C++" iş yükü)

Ardından:

```powershell
cargo build --release
```

Çıktı: `target/release/flyout-lite.exe`

## Test edildiği ortam

- **Windows 11 25H2** (yazarın makinesi) — tamamen çalışıyor.

Hepsi bu. FlyoutLite, daha eski Windows 11 sürümlerinde, Windows 10'da, ARM64'te veya birincil ekran dışındaki çoklu monitör düzenlerinde test edilmedi. **Farklı bir kurulumda çalıştırırsanız ve bir şey çalışırsa (ya da çalışmazsa) lütfen bir issue açın** — raporlar ve PR'lar her zaman memnuniyetle karşılanır, özellikle 25H2 dışındaki sürümler için.

## Bilinen kısıtlamalar

- Yalnızca birincil monitör (çoklu monitör konumlandırması henüz uygulanmadı).
- Ayarlar penceresindeki metin alanlarında henüz yanıp sönen imleç yok — yalnızca odak çerçevesi var. Rakam yazımı ve backspace çalışıyor.
- Bazı oynatıcılar timeline verisi raporlamıyor (konum/süre yok); bu durumda seek çubuğu `-:--` gösterir ama yine de tıklayınca atlatma denenir.

## Lisans

[MIT](LICENSE) © 2026 Ahmed Cemil BİLGİN
