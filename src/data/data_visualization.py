import matplotlib.pyplot as plt

# Data to plot
categories = ['Unique IPs', 'Unique Days', 'Unique Months', 'Unique Years', 'Unique Times', 
              'Unique Offsets', 'Unique Methods', 'Unique URLs', 'Unique Protocols', 
              'Unique Status Codes', 'Unique Response Sizes']
values = [11, 31, 4, 1, 2005, 1, 2, 206, 1, 6, 382]

# Create bar chart
plt.figure(figsize=(14, 8))
plt.bar(categories, values, color='skyblue')
plt.title('Unique Values in Access Log')
plt.xlabel('Categories')
plt.ylabel('Count')
plt.xticks(rotation=45, ha="right")
plt.tight_layout()

# Display the chart
plt.show()